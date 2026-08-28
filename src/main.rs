#![windows_subsystem = "windows"]

mod runtime;

use windows_reactor::*;

fn app(_cx: &mut RenderCx) -> Element {
    text_block("Hello, world!").font_size(32.0).bold().into()
}

fn main() -> Result<()> {
    runtime::ensure_wasdk_runtime()?;
    bootstrap()?;
    App::new()
        .title("KumoRust")
        .inner_size(480.0, 260.0)
        .render(app)
}
