use bitcoin::hashes::Hash;
use bitcoin::hashes::hex::ToHex;
use bitcoin::hashes::sha256::Hash as Sha256;

use tonic_openssl_lnd::LndRouterClient;

pub async fn start_htlc_event_subscription(mut lnd: LndRouterClient) {
    let mut htlc_event_stream = lnd
        .subscribe_htlc_events(tonic_openssl_lnd::routerrpc::SubscribeHtlcEventsRequest {})
        .await
        .expect("Failed to start htlc event subscription")
        .into_inner();

    while let Some(htlc_event) = htlc_event_stream
        .message()
        .await
        .expect("Failed to receive htlc events")
    {
        if let Some(tonic_openssl_lnd::routerrpc::htlc_event::Event::SettleEvent(settle_event)) =
            htlc_event.event
        {
            let payment_hash = Sha256::hash(settle_event.preimage.as_slice());
            println!(
                "got preimage {} from payment hash {}",
                settle_event.preimage.clone().to_hex(),
                payment_hash.to_hex()
            );
        };
    }
}

#[allow(unused)]
enum HtlcInterceptorAction {
    /// Settle the HTLC with the given preimage
    Settle = 0,
    /// Fail the HTLC with the given failure code and message
    Fail = 1,
    /// Allow lnd to make the decision on the HTLC
    Resume = 2,
}

pub async fn start_htlc_interceptor(lnd: LndRouterClient) {
    let mut router = lnd.clone();
    let (tx, rx) = tokio::sync::mpsc::channel::<
        tonic_openssl_lnd::routerrpc::ForwardHtlcInterceptResponse,
    >(1024);
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);

    let mut htlc_stream = router
        .htlc_interceptor(stream)
        .await
        .expect("Failed to start htlc interceptor")
        .into_inner();

    while let Some(htlc) = htlc_stream
        .message()
        .await
        .expect("Failed to receive HTLCs")
    {
        println!("Received HTLC {}!", htlc.payment_hash.to_hex());

        let response = tonic_openssl_lnd::routerrpc::ForwardHtlcInterceptResponse {
            incoming_circuit_key: htlc.incoming_circuit_key,
            action: HtlcInterceptorAction::Resume as i32,
            preimage: vec![],
            failure_code: 0,
            failure_message: vec![],
        };

        tx.send(response).await.unwrap();
    }
}
