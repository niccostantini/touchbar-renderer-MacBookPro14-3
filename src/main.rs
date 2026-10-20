fn main() {
    // Started by the t1bridge launcher without arguments, as the logged-in
    // user. Volume and media are handled directly, without the desktop
    // provider the built-in renderer uses.
    touchbar_renderer::session::run_forever();
}
