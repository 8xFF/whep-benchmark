use dioxus::prelude::*;
use dioxus_tui::Config;

mod whip;

fn main() {
    dioxus_tui::launch_cfg(app, Config::new());
}

fn app(cx: Scope) -> Element {
    let name = use_state(cx, || "bob".to_string());

    cx.render(rsx! {
        div{
            width: "100%",
            flex_direction: "column",

            input {
                // and what to do when the value changes
                oninput: move |evt| name.set(evt.value.clone()),
            }
            div {
                flex_grow: 1,
                overflow: "hidden",
                ul {
                    flex_direction: "column",
                    (0..1000).map(|i| rsx!(div {
                        li {
                            padding_right: "3px",

                            "Sender {i}"
                        }
                        li {
                            padding_right: "3px",

                            "10Mbps/1Mbps"
                        }
                        li {
                            padding_right: "3px",

                            "Running"
                        }
                    }))
                }
            }
        }
    })
}