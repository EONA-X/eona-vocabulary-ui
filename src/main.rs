mod components;
mod ontology;

use yew::prelude::*;

#[function_component(App)]
fn app() -> Html {
    html! {
        <main class="app">
            <header class="app-header">
                <h1>{ "EOVoc UI" }</h1>
            </header>
            <p>{ "Yew skeleton — ontology browser migration in progress." }</p>
        </main>
    }
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    yew::Renderer::<App>::new().render();
}
