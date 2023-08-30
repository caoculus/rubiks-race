use leptos::*;
use leptos_meta::*;
use leptos_router::*;

#[cfg(not(feature = "ssr"))]
mod game;
#[cfg(not(feature = "ssr"))]
use game::Game;

#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
        // injects a stylesheet into the document <head>
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href="/pkg/start-axum.css"/>

        // sets the document title
        <Title text="Rubik's Race"/>

        // content for this welcome page
        <Router>
            <main>
                <Routes>
                    <Route path="" view=|| view! { <HomePage/> }/>
                    <Route path="/game" view=|| view! { <Game/> }/>
                </Routes>
            </main>
        </Router>
    }
}

#[cfg(feature = "ssr")]
#[component]
fn Game() -> impl IntoView {
    let (dimensions, _) = create_signal((0, 0));
    game_view(dimensions, None::<()>, None::<()>, None::<()>, None::<()>)
}

/// Renders the home page of your application.
#[component]
fn HomePage() -> impl IntoView {
    // using Form is a workaround for a redirecting button
    view! {
        <div class="home">
            <h1>"Rubik's Race"</h1>
            <Form method="GET" action="/game">
                <button class="button">"Play"</button>
            </Form>
        </div>
    }
}
