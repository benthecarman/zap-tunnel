use crate::models::*;
use gloo_net::http::Request;
use leptos::html::Input;
use leptos::*;
use leptos_meta::*;
use leptos_router::*;

#[cfg(feature = "ssr")]
pub fn register_server_functions() {}

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
    let all = create_local_resource(
        cx,
        move || {},
        |_| async move {
            tracing::info!("loading data...");
            let resp = Request::get("/all").send().await.unwrap();

            resp.json::<Vec<SetupUser>>().await.unwrap()
        },
    );

    let action = create_action(cx, |user: &SetupUser| {
        let user = user.to_owned();
        async move {
            let resp = Request::post(&format!("/setup-user"))
                .json(&user)
                .unwrap()
                .send()
                .await
                .unwrap();

            resp.status()
        }
    });

    let username_ref = create_node_ref::<Input>(cx);
    let proxy_ref = create_node_ref::<Input>(cx);

    view! { cx,
        <h1>"Welcome to Zap Tunnel"</h1>
        <p>"This allows you to have a lightning address that goes right your lightning node!."</p>
        <br/>
        <Suspense fallback=move || view! { cx, <p>"Loading..."</p> }>
            { move || all.read(cx).map(|all|
                all.iter().map(|status| view! { cx, <pre><a href=format!("/view/{}", &status.proxy)>{&status.username} {&status.proxy}</a></pre> }).collect::<Vec<_>>())
            }
        </Suspense>
                    <ErrorBoundary
                // the fallback receives a signal containing current errors
                fallback=|cx, errors| view! { cx,
                    <div class="error">
                        <p>"Not a number! Errors: "</p>
                        // we can render a list of errors
                        // as strings, if we'd like
                        <ul>
                            {move || errors.get()
                                .into_iter()
                                .map(|(_, e)| view! { cx, <li>{e.to_string()}</li>})
                                .collect::<Vec<_>>()
                            }
                        </ul>
                    </div>
                }
            >
        <form on:submit=move |ev| {
            ev.prevent_default();
            let username = username_ref.get().unwrap();
            let proxy = proxy_ref.get().unwrap();
            let user = SetupUser {
                username: username.value(),
                proxy: proxy.value(),
            };
            action.dispatch(user);
        }>
            <input type="text" name="username" placeholder="name"  node_ref=username_ref/>
            <input type="text" name="proxy" placeholder="tbc" node_ref=proxy_ref />
            <input type="submit" value="Submit!" />
        </form>
        <p> {move || action.pending().get().then_some("Loading...") } </p>
        <p> {move || action.value().get().unwrap_or_default().to_string()} </p>
        </ErrorBoundary>
    }
}

#[derive(Params, PartialEq, Clone, Debug)]
pub struct ServiceParams {
    service_name: String,
}

#[component]
fn ServiceViewer(cx: Scope) -> impl IntoView {
    let params = use_params::<ServiceParams>(cx);

    let status_data = create_local_resource(
        cx,
        move || params().unwrap().service_name.clone(),
        |proxy| async move {
            tracing::info!("loading data...");
            let resp = Request::get(&format!("/status/{proxy}"))
                .send()
                .await
                .unwrap();

            resp.json::<Status>().await.unwrap()
        },
    );

    view! { cx,
        <h1>{params().ok().unwrap().service_name}</h1>
        <Suspense fallback=move || view! { cx, <p>"Loading..."</p> }>
            { move || status_data.read(cx).map(|status| view! { cx, <pre>{status.username} {status.proxy} {status.invoices_remaining}</pre> }) }
        </Suspense>
    }
}
