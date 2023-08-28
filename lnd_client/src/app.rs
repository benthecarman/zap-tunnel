use crate::models::*;
use gloo_net::http::Request;
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
        <Stylesheet id="leptos" href="/pkg/zap_tunnel_lnd.css"/>

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

    view! { cx,
        <h1>"Welcome to Zap Tunnel"</h1>
        <p>"This allows you to have a lightning address that goes right your lightning node!"</p>
        <br/>
        <Suspense fallback=move || view! { cx, <p>"Loading..."</p> }>
            { move || all.read(cx).map(|all| view!{ cx,
                <div>
                    <h2>"Current registrations"</h2>
                    <table>
                        <tr>
                            <th>"Username"</th>
                            <th>"Proxy"</th>
                        </tr>
                        { all.iter().map(|status| view! { cx, <tr><td>{&status.username}</td><td>{&status.proxy}</td><td><a href=format!("/view/{}", status.proxy)><button>View</button></a></td></tr> }).collect_view(cx) }
                    </table>
                </div>
            })
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
        <Form method="POST" action="setup-user">
            <input type="text" name="username" placeholder="name" />
            <input type="text" name="proxy" placeholder="zaptunnel.com" />
            <input type="submit" value="Submit!" />
        </Form>
        </ErrorBoundary>
    }
}

#[derive(Params, PartialEq, Clone, Debug)]
pub struct ServiceParams {
    service_name: Option<String>,
}

#[component]
fn ServiceViewer(cx: Scope) -> impl IntoView {
    let params = use_params::<ServiceParams>(cx);

    let status_data = create_local_resource(
        cx,
        || (),
        move |_| async move {
            tracing::info!("loading data...");
            let proxy = params.get().unwrap().service_name.unwrap_or_default();
            let resp = Request::get(&format!("/status/{proxy}"))
                .send()
                .await
                .unwrap();

            resp.json::<Status>().await.unwrap()
        },
    );

    view! { cx,
        <h1>{params.get().ok().unwrap().service_name}</h1>
        <Suspense fallback=move || view! { cx, <p>"Loading..."</p> }>
            { move || status_data.read(cx).map(|status| view! { cx,
                <pre>{format!("Username: {}", status.username)}</pre>
                <pre>{format!("Invoices Remaining: {}", status.invoices_remaining)}</pre>
            })}
        </Suspense>
    }
}
