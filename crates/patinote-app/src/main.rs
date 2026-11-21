//! The `patinote-app` binary: the Iced desktop app.

use patinote_app::app::{App, Launch};
use patinote_app::{args, fonts, session::VaultPaths, theme};
use patinote_core::config::{Renderer, global_config_path, load_global};
use patinote_core::ipc::{self, Request};
use patinote_core::location;

/// Set to `1` to print how many font faces Iced loaded and exit; used by the
/// test that checks system fonts are not scanned.
const FONT_REPORT: &str = "PATINOTE_FONT_REPORT";

fn main() -> std::process::ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::WARN)
        .without_time()
        .init();

    let args = match args::parse(std::env::args_os().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("patinote-app: {e}");
            eprintln!("usage: patinote-app [--vault PATH] [--open NOTE]");
            return std::process::ExitCode::from(2);
        }
    };
    let config = match location::config_home().map(|h| load_global(&global_config_path(&h))) {
        Some(Ok(c)) => c,
        Some(Err(e)) => {
            eprintln!("patinote-app: {e}");
            Default::default()
        }
        None => Default::default(),
    };
    for w in &config.warnings {
        eprintln!("patinote-app: config: {w}");
    }
    let (Some(cache_home), Some(state_home)) = (location::cache_home(), location::state_home())
    else {
        eprintln!("patinote-app: HOME is not set");
        return std::process::ExitCode::FAILURE;
    };
    let runtime_dir = ipc::runtime_dir();
    let vault = args
        .vault
        .clone()
        .or_else(|| std::env::var_os("PATINOTE_VAULT").map(Into::into))
        .or_else(|| {
            let last: patinote_app::state::AppState =
                patinote_app::state::load(&patinote_app::state::app_state_path(&state_home));
            last.last_vault
        })
        .or_else(|| config.vault_default.clone());

    // One app per vault: hand the request to a running one and stop.
    if let (Some(dir), Some(vault)) = (&runtime_dir, &vault)
        && let Ok(paths) = VaultPaths::resolve(vault, &cache_home, &state_home)
    {
        let request = match &args.open {
            Some(path) => Request::Open { path: path.clone() },
            None => Request::Activate,
        };
        if let Ok(answer) = ipc::send(&ipc::socket_path(dir, &paths.id), &request)
            && answer.ok
        {
            return std::process::ExitCode::SUCCESS;
        }
    }

    let backend = config.renderer.unwrap_or(Renderer::TinySkia).name();
    if let Err(reason) = fonts::restart_without_system_fonts(backend) {
        eprintln!("patinote-app: system fonts are scanned: {reason}");
    }
    if std::env::var_os(FONT_REPORT).is_some() {
        let mut system = iced::advanced::graphics::text::font_system()
            .write()
            .expect("the font system lock is never poisoned before the UI starts");
        for font in fonts::FONTS {
            system.load_font(std::borrow::Cow::Borrowed(font));
        }
        println!("{}", system.raw().db().len());
        return std::process::ExitCode::SUCCESS;
    }

    let size = patinote_app::app::window_size(&state_home);
    let launch = Launch {
        vault,
        note: args.open,
        config,
        cache_home,
        state_home,
        runtime_dir,
        watch: true,
    };
    let mut app = iced::application(move || App::new(launch.clone()), App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .theme(|_: &App| theme::theme())
        .scale_factor(App::zoom)
        .default_font(fonts::DEFAULT)
        .exit_on_close_request(false)
        .window_size(size);
    for font in fonts::FONTS {
        app = app.font(font);
    }
    match app.run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("patinote-app: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
