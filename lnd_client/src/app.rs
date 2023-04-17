use leptos::*;
use leptos_meta::*;
use leptos_router::*;

#[cfg(feature = "ssr")]
pub fn register_server_functions() {
    _ = SetupUser::register();
    _ = ServiceStatus::register();
}

#[server(SetupUser, "/api")]
pub async fn setup_user(name: String, service: String) -> Result<String, ServerFnError> {
    tracing::info!("Setting up user...");

    let db: sled::Db = sled::open("profiles.sled").unwrap();

    let old_value = db.insert(service.as_bytes(), name.as_bytes()).unwrap();

    Ok(format!("old values: {:?}", old_value))
}

#[server(ServiceStatus, "/api")]
pub async fn service_status(service_name: String) -> Result<String, ServerFnError> {
    Ok(format!("hey this is the status of {service_name}"))
}

#[component]
pub fn App(cx: Scope) -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context(cx);

    view! {
        cx,

        // injects a stylesheet into the document <head>
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href="/pkg/leptos_start.css"/>

        // sets the document title
        <Title text="Zap Tunnel"/>

        // content for this welcome page
        <Router>
            <main>
                <Routes>
                    <Route path="" view=|cx| view! { cx, <HomePage/> }/>
                    <Route path="/view/:service_name" view=|cx| view! { cx, <ServiceViewer /> }/>
                </Routes>
            </main>
        </Router>
    }
}

/// Renders the home page of your application.
#[component]
fn HomePage(cx: Scope) -> impl IntoView {
    let setup = create_server_action::<SetupUser>(cx);

    // let status_data = create_resource(
    //     cx,
    //     move || params().unwrap().service_name.clone(),
    //     |value| async move {
    //         tracing::info!("loading data...");
    //         service_status(value).await
    //     },
    // );

    view! { cx,
        <h1>"Welcome to Zap Tunnel"</h1>
        <p>"This allows you to have a lightning address that goes right your lightning node!."</p>
        <br/>
        <ActionForm action=setup>
            <input type="text" name="name" placeholder="name" />
            <input type="text" name="service" placeholder="tbc" />
            <input type="submit" value="Submit!" />
        </ActionForm>
    }
}

#[derive(Params, PartialEq, Clone, Debug)]
pub struct ServiceParams {
    service_name: String,
}

#[component]
fn ServiceViewer(cx: Scope) -> impl IntoView {
    let params = use_params::<ServiceParams>(cx);

    let status_data = create_resource(
        cx,
        move || params().unwrap().service_name.clone(),
        |value| async move {
            tracing::info!("loading data...");
            service_status(value).await
        },
    );

    view! { cx,
        <h1>{params().ok().unwrap().service_name}</h1>
        <Suspense fallback=move || view! { cx, <p>"Loading..."</p> }>
            { move || status_data.read(cx).map(|status| view! { cx, <pre>{status}</pre> }) }
        </Suspense>
    }
}
