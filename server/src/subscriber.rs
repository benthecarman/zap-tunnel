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
                    handle_open_hodl_invoice(
                        ln_invoice,
                        router.clone(),
                        invoice_client.clone(),
                        &config,
                        db_pool.clone(),
                    )
                    .await
                }
            }
            Some(InvoiceState::Accepted) => {
                handle_accepted_invoice(
                    ln_invoice,
                    router.clone(),
                    invoice_client.clone(),
                    &config,
                    db_pool.clone(),
                )
                .await
            }
            None | Some(InvoiceState::Canceled) | Some(InvoiceState::Settled) => {}
        }
    }
}

async fn handle_open_hodl_invoice(
    inv: lnrpc::Invoice,
    router: LndRouterClient,
    mut invoice_client: LndInvoicesClient,
    config: &Config,
    db_pool: Pool<ConnectionManager<SqliteConnection>>,
) {
    println!("got open hodl invoice: {:?}", inv.payment_request);

    let req = SubscribeSingleInvoiceRequest { r_hash: inv.r_hash };
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
    mut router: LndRouterClient,
    mut invoice_client: LndInvoicesClient,
    config: &Config,
    db_pool: Pool<ConnectionManager<SqliteConnection>>,
) {
    println!("got accepted invoice: {:?}", ln_invoice.payment_request);

    let db = &mut db_pool.get().expect("Failed to get db connection");

    let invoice_hash: Vec<u8> = ln_invoice.r_hash;

    let invoice_opt: Option<Invoice> = dsl::invoices
        .filter(payment_hash.eq(invoice_hash.to_hex()))
        .filter(paid.eq(0))
        .first::<Invoice>(db)
        .optional()
        .ok()
        .flatten();

    if let Some(user_invoice) = invoice_opt {
        let invoice_time = user_invoice.invoice().timestamp();
        let expiry_time = user_invoice.invoice().expiry_time();
        let timeout_seconds = match invoice_time.elapsed() {
            Ok(elapsed) => {
                let remaining_time_secs = expiry_time
                    .checked_sub(elapsed)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                // max 60 seconds timeout, min 10 seconds timeout
                if remaining_time_secs > 60 {
                    Some(60)
                } else if remaining_time_secs > 10 {
                    Some(remaining_time_secs)
                } else {
                    None
                }
            }
            Err(_) => None,
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

            let mut stream = router
                .send_payment_v2(req)
                .await
                .expect("Failed to send payment")
                .into_inner();

            if let Some(payment) = stream.message().await.ok().flatten() {
                if let Some(PaymentStatus::Succeeded) = PaymentStatus::from_i32(payment.status) {
                    // success
                    println!("paid invoice: {:?}", ln_invoice.payment_request);

                    let preimage: Vec<u8> = Vec::from_hex(payment.payment_preimage.as_str())
                        .expect("Failed to parse preimage");

                    // settle invoice
                    invoice_client
                        .settle_invoice(invoicesrpc::SettleInvoiceMsg { preimage })
                        .await
                        .expect("Failed to settle invoice");

                    // mark invoice as paid
                    Invoice::mark_invoice_paid(&invoice_hash.to_hex(), db)
                        .expect("Failed to mark invoice as paid");

                    // create and broadcast zap if applicable
                    // todo handle errors
                    handle_zap(&invoice_hash, &config.nostr_keys(), db)
                        .await
                        .expect("Failed to handle zap");
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
                        .await
                        .expect("Failed to cancel invoice");

                    println!("cancelled invoice: {}", invoice_hash.to_hex());
                }
            }
        }
    }
}
