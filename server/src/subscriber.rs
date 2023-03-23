use bitcoin::hashes::hex::ToHex;
use diesel::associations::HasTable;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, SqliteConnection};
use nostr::Keys;
use tonic_openssl_lnd::lnrpc::InvoiceSubscription;
use tonic_openssl_lnd::{LndInvoicesClient, LndLightningClient, LndRouterClient};

use crate::models::invoice::Invoice;
use crate::models::schema::invoices::*;
use crate::nostr::handle_zap;

#[allow(unused)]
enum InvoiceState {
    /// The invoice has been created, but no htlc has been received yet.
    Open = 0,
    /// The invoice has been paid.
    Settled = 1,
    /// The invoice has been canceled by the user.
    Canceled = 2,
    /// The invoice has been accepted but waiting for a preimage to settle.
    Accepted = 3,
}

impl InvoiceState {
    fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(InvoiceState::Open),
            1 => Some(InvoiceState::Settled),
            2 => Some(InvoiceState::Canceled),
            3 => Some(InvoiceState::Accepted),
            _ => None,
        }
    }
}

pub async fn start_invoice_subscription(
    mut lnd: LndLightningClient,
    mut router: LndRouterClient,
    mut invoice_client: LndInvoicesClient,
    nostr_key: Keys,
    db_pool: Pool<ConnectionManager<SqliteConnection>>,
) {
    println!("Starting invoice subscription");

    let sub = InvoiceSubscription {
        add_index: 0,
        settle_index: 0,
    };
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
        if let Some(InvoiceState::Accepted) = InvoiceState::from_i32(ln_invoice.state) {
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

            if let Some(mut user_invoice) = invoice_opt {
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
                    // todo fee limits
                    let req = tonic_openssl_lnd::routerrpc::SendPaymentRequest {
                        payment_request: user_invoice.invoice().to_string(),
                        amt_msat: ln_invoice.value_msat,
                        timeout_seconds: timeout_seconds as i32,
                        no_inflight_updates: true,
                        final_cltv_delta: (ln_invoice.cltv_expiry as i32 - 3).min(1),
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
                        if payment.status == 2 {
                            // success
                            println!("paid invoice: {:?}", ln_invoice.payment_request);

                            // settle invoice
                            invoice_client
                                .settle_invoice(tonic_openssl_lnd::invoicesrpc::SettleInvoiceMsg {
                                    preimage: Vec::from(payment.payment_preimage), // todo is this correct?
                                })
                                .await
                                .expect("Failed to settle invoice");

                            // mark invoice as paid
                            user_invoice.set_paid();
                            diesel::update(dsl::invoices::table())
                                .set(user_invoice)
                                .execute(db)
                                .expect("Failed to mark invoice as paid");

                            // create and broadcast zap if applicable
                            // todo handle errors
                            handle_zap(invoice_hash, nostr_key.clone(), db)
                                .await
                                .expect("Failed to handle zap");
                        } else {
                            // failed or unknown
                            println!("failed to pay invoice: {:?}", ln_invoice.payment_request);
                        }
                    }
                }
            }
        }
    }
}
