fn main() {
    // fluent supports Palette.color-scheme (dark/light). Lock look via dark in app.slint init;
    // light theme later = set ColorScheme.light without changing this style name.
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint UI compile failed");
}
