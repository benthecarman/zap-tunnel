use anyhow::anyhow;
use bitcoin::hashes::hex::{FromHex, ToHex};
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, SqliteConnection};
use lnrpc::payment::PaymentStatus;
use tonic_openssl_lnd::invoicesrpc::SubscribeSingleInvoiceRequest;
use tonic_openssl_lnd::lnrpc::invoice::InvoiceState;
use tonic_openssl_lnd::{
    invoicesrpc, lnrpc, LndInvoicesClient, LndLightningClient, LndRouterClient,
};

use crate::config::Config;
use crate::models::invoice::Invoice;
use crate::models::schema::invoices::*;
use crate::nostr::handle_zap;

pub async fn start_active_invoice_subscriptions(
    router: LndRouterClient,
    invoice_client: LndInvoicesClient,
    config: Config,
    db_pool: Pool<ConnectionManager<SqliteConnection>>,
) -> anyhow::Result<()> {
    let db = &mut db_pool.get()?;

    let active_invoices = Invoice::get_active_invoices(db)?;

    println!(
        "Starting active invoice subscriptions: {:?}",
        active_invoices.len()
    );

    for inv in active_invoices.iter() {
        let r_hash = inv.payment_hash().to_vec();
        let router_clone = router.clone();
        let invoice_client_clone = invoice_client.clone();
        let config_clone = config.clone();
        let db_pool_clone = db_pool.clone();

        // Use tokio::spawn instead of tokio::task::spawn
        // to avoid borrowing the variables beyond their lifetime.
        tokio::spawn(async move {
            handle_open_hodl_invoice(
                r_hash,
                router_clone,
                invoice_client_clone,
                &config_clone,
                db_pool_clone,
            )
            .await;
        });
    }

    Ok(())
}

pub async fn start_invoice_subscription(
    mut lnd: LndLightningClient,
    router: LndRouterClient,
    invoice_client: LndInvoicesClient,
    config: Config,
    db_pool: Pool<ConnectionManager<SqliteConnection>>,
) {
    println!("Starting invoice subscription");

    let sub = lnrpc::InvoiceSubscription::default();
    let mut invoice_stream = lnd
        .subscribe_invoices(sub)
        .await
        .expect("Failed to start invoice subscription")
        .into_inner();

    while let Some(ln_invoice) = invoice_stream
        .message()
        .await
        .expect("Failed to receive invoices")
    {
        match InvoiceState::from_i32(ln_invoice.state) {
            Some(InvoiceState::Open) => {
                if ln_invoice.r_preimage.is_empty() {
                    let invoice_client = invoice_client.clone();
                    let router = router.clone();
                    let config = config.clone();
                    let db_pool = db_pool.clone();
                    tokio::spawn(async move {
                        handle_open_hodl_invoice(
                            ln_invoice.r_hash,
                            router,
                            invoice_client,
                            &config,
                            db_pool,
                        )
                        .await
                    });
                }
            }
            Some(InvoiceState::Accepted) => {
                let invoice_client = invoice_client.clone();
                let router = router.clone();
                let config = config.clone();
                let db_pool = db_pool.clone();
                tokio::spawn(async move {
                    handle_accepted_invoice(ln_invoice, router, invoice_client, &config, db_pool)
                        .await
                });
            }
            None | Some(InvoiceState::Canceled) | Some(InvoiceState::Settled) => {}
        }
    }
}

async fn handle_open_hodl_invoice(
    r_hash: Vec<u8>,
    router: LndRouterClient,
    mut invoice_client: LndInvoicesClient,
    config: &Config,
    db_pool: Pool<ConnectionManager<SqliteConnection>>,
) {
    println!("got open hodl invoice: {}", r_hash.to_hex());

    let req = SubscribeSingleInvoiceRequest { r_hash };
    let mut invoice_stream = invoice_client
        .subscribe_single_invoice(req)
        .await
        .expect("Failed to subscribe to single invoice")
        .into_inner();

    while let Some(ln_invoice) = invoice_stream
        .message()
        .await
        .expect("Failed to receive single invoice stream")
    {
        if let Some(InvoiceState::Accepted) = InvoiceState::from_i32(ln_invoice.state) {
            handle_accepted_invoice(
                ln_invoice,
                router.clone(),
                invoice_client.clone(),
                config,
                db_pool.clone(),
            )
            .await
        }
    }
}

async fn handle_accepted_invoice(
    ln_invoice: lnrpc::Invoice,
    router: LndRouterClient,
    mut invoice_client: LndInvoicesClient,
    config: &Config,
    db_pool: Pool<ConnectionManager<SqliteConnection>>,
) {
    let result = handle_accepted_invoice_impl(
        ln_invoice.clone(),
        router,
        invoice_client.clone(),
        config,
        db_pool,
    )
    .await;

    // Cancel invoice if there was an error
    // otherwise the invoice will stay in the accepted state
    // and cause a stuck payment.
    if let Err(e) = result {
        println!("Error handling accepted invoice: {:?}", e);
        let invoice_hash: Vec<u8> = ln_invoice.r_hash;

        invoice_client
            .cancel_invoice(invoicesrpc::CancelInvoiceMsg {
                payment_hash: invoice_hash.to_vec(),
            })
            .await
            .expect("Failed to cancel invoice");

        println!("cancelled invoice: {}", invoice_hash.to_hex());
    }
}

async fn handle_accepted_invoice_impl(
    ln_invoice: lnrpc::Invoice,
    mut router: LndRouterClient,
    mut invoice_client: LndInvoicesClient,
    config: &Config,
    db_pool: Pool<ConnectionManager<SqliteConnection>>,
) -> anyhow::Result<()> {
    println!("got accepted invoice: {}", ln_invoice.r_hash.to_hex());

    let db = &mut db_pool.get()?;

    let invoice_hash: Vec<u8> = ln_invoice.r_hash;

    let invoice_opt: Option<Invoice> = dsl::invoices
        .filter(payment_hash.eq(invoice_hash.to_hex()))
        .filter(fees_earned.is_null())
        .first::<Invoice>(db)
        .optional()
        .ok()
        .flatten();

    if let Some(user_invoice) = invoice_opt {
        let remaining_time_secs = user_invoice.invoice().duration_until_expiry().as_secs();
        // max 60 seconds timeout, min 10 seconds timeout
        let timeout_seconds = if remaining_time_secs > 60 {
            Some(60)
        } else if remaining_time_secs > 10 {
            Some(remaining_time_secs)
        } else {
            None
        };

        // only pay invoice if we have enough time
        if let Some(timeout_seconds) = timeout_seconds {
            let total_fee =
                config.base_fee as f64 + (config.fee_rate / 100.0) * ln_invoice.value_msat as f64;
            let total_fee = total_fee as i64;

            let amt_msat: i64 = ln_invoice.value_msat - total_fee;
            let req = tonic_openssl_lnd::routerrpc::SendPaymentRequest {
                payment_request: user_invoice.invoice,
                amt_msat,
                fee_limit_msat: total_fee,
                timeout_seconds: timeout_seconds as i32,
                no_inflight_updates: true,
                allow_self_payment: false,
                amp: false,
                time_pref: 0.9,
                ..Default::default()
            };

            let mut stream = router.send_payment_v2(req).await?.into_inner();

            if let Some(payment) = stream.message().await.ok().flatten() {
                if let Some(PaymentStatus::Succeeded) = PaymentStatus::from_i32(payment.status) {
                    // success
                    println!("paid invoice: {}", invoice_hash.to_hex());

                    let preimage: Vec<u8> = Vec::from_hex(payment.payment_preimage.as_str())?;

                    // settle invoice
                    invoice_client
                        .settle_invoice(invoicesrpc::SettleInvoiceMsg { preimage })
                        .await?;

                    let fees_earned_msats = total_fee - payment.fee_msat;

                    // mark invoice as paid
                    Invoice::mark_invoice_paid(&invoice_hash.to_hex(), fees_earned_msats, db)
                        .expect("Failed to mark invoice as paid");

                    // create and broadcast zap if applicable
                    // todo handle errors
                    handle_zap(&invoice_hash, &config.nostr_keys(), db)
                        .await
                        .expect("Failed to handle zap");

                    return Ok(());
                } else {
                    // failed or unknown
                    println!(
                        "failed to pay invoice ({}) {}: {}",
                        payment.status,
                        invoice_hash.to_hex(),
                        payment.failure_reason
                    );

                    invoice_client
                        .cancel_invoice(invoicesrpc::CancelInvoiceMsg {
                            payment_hash: invoice_hash.to_vec(),
                        })
                        .await?;

                    println!("cancelled invoice: {}", invoice_hash.to_hex());
                    return Ok(());
                }
            }
        }
    }

    Err(anyhow!("Failed to handle invoice"))
}
