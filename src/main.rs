use anyhow::{Context, Result};
use psst::{
    cli::{Args, HELP},
    config::{Config, discover},
    console, materialization, operations, providers, push, ui,
};
use std::{ffi::OsString, path::PathBuf, process::ExitCode};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            console::error(format_args!("psst: {e:#}"));
            ExitCode::FAILURE
        }
    }
}
async fn run() -> Result<u8> {
    let args = Args::parse(std::env::args_os().skip(1))?;
    if args.version {
        println!("psst {}", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }
    if args.help && args.copy.is_none() && !args.write {
        print!("{HELP}");
        return Ok(0);
    }
    let cwd = std::env::current_dir()?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let config = Config::load(discover(
        &cwd,
        home.as_deref(),
        std::env::var_os("PSST_CONFIG").as_deref(),
    ))?;
    if args.copy.is_none()
        && !args.write
        && !args.files_pull
        && !args.files_push
        && args.command.is_empty()
    {
        ui::run(config).await?;
        return Ok(0);
    }
    let operation = async {
        if args.files_push {
            return Ok(if push::run(&config, args.yes).await? {
                0
            } else {
                1
            });
        }
        if args.files_pull {
            return Ok(if materialization::run(&config, args.yes).await? {
                0
            } else {
                1
            });
        }
        if let Some(name) = args.copy {
            let name = name.to_str().context("Secret name is not UTF8")?;
            let entry = config
                .entries
                .get(name)
                .filter(|entry| entry.kind == psst::config::MappingKind::Env)
                .with_context(|| format!("Secret {name:?} not found"))?;
            let value = providers::fetch(entry, config.dir()).await?;
            operations::copy(&value).await?;
            console::info(format_args!("📋 Secret {name} copied to clipboard"));
            return Ok(0);
        }
        let results = providers::all(&config.entries, config.dir()).await;
        for (name, result) in &results {
            if let Err(e) = result {
                console::warn(format_args!("psst: {name}: {e}"));
            }
        }
        for (name, result) in &results {
            if config.entries[name].source == "local-storage" && result.is_err() {
                anyhow::bail!("Local-storage resolution failed; operation stopped");
            }
        }
        if args.write {
            operations::write_env(&cwd.join(".env"), &results)?;
            console::info(format_args!(
                "📝 Created .env (plaintext, permissions 0600)"
            ));
            return Ok(0);
        }
        let count = results.iter().filter(|(_, r)| r.is_ok()).count();
        if count == 0 {
            console::info(format_args!("💉 No secrets injected"));
        } else {
            console::info(format_args!(
                "💉 injected {count} secret{}",
                if count == 1 { "" } else { "s" }
            ));
        }
        launch(args.command, results)
    };
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = operation => result,
            _ = tokio::signal::ctrl_c() => {
                if args.files_push { console::warn(format_args!("Push cancelled; unfinished files failed; completed writes, if any, remain.")); }
                Ok(130)
            },
            _ = term.recv() => {
                if args.files_push { console::warn(format_args!("Push cancelled; unfinished files failed; completed writes, if any, remain.")); }
                Ok(143)
            },
        }
    }
    #[cfg(not(unix))]
    {
        tokio::select! { result = operation => result, _ = tokio::signal::ctrl_c() => Ok(130) }
    }
}
fn launch(
    argv: Vec<OsString>,
    results: Vec<(String, Result<zeroize::Zeroizing<String>>)>,
) -> Result<u8> {
    let mut child = std::process::Command::new(&argv[0]);
    child.args(&argv[1..]);
    for (name, result) in results {
        if let Ok(value) = result {
            child.env(name, value.as_str());
        }
    }
    // Replace the wrapper on Unix: same PID, terminal/job control, exact signal semantics.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Tokio installed signal handlers while resolving. exec resets caught handlers.
        Err(child.exec()).context("Cannot execute wrapped command")
    }
    #[cfg(not(unix))]
    {
        let status = child.status().context("Cannot execute wrapped command")?;
        Ok(status.code().unwrap_or(1) as u8)
    }
}
