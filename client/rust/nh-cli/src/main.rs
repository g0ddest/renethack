//! nh-cli: run the engine with a script, record and replay sessions.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, Subcommand};
use nh_link::*;
use nh_protocol::{EngineMsg, Request, WinCall, WindowKind, parse_line};

#[derive(Parser)]
#[command(about = "Drive nh-engine from the command line")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Play a script and print what the game showed.
    Run(RunArgs),
    /// Re-run a recording and check the engine output is identical.
    Replay {
        #[command(flatten)]
        engine: EngineArgs,
        /// Recording written by `run --record`.
        recording: PathBuf,
    },
    /// Print a summary of the engine's catalog.
    Catalog {
        #[command(flatten)]
        engine: EngineArgs,
    },
}

#[derive(Args)]
struct EngineArgs {
    /// Directory with nh-engine and data/ (default: $RENETHACK_ENGINE_DIR, else the
    /// engine/build of this checkout).
    #[arg(long)]
    engine_dir: Option<PathBuf>,
    /// Playground directory to use (default: a fresh temporary one).
    #[arg(long)]
    playground: Option<PathBuf>,
}

#[derive(Args)]
struct RunArgs {
    #[command(flatten)]
    engine: EngineArgs,
    /// Script file (see nh_link::parse_script).
    script: PathBuf,
    /// NETHACKOPTIONS (default: a neutral human valkyrie named Hero).
    #[arg(long)]
    options: Option<String>,
    #[arg(long)]
    seed: Option<u64>,
    /// Fixed clock, unix seconds.
    #[arg(long)]
    fixed_time: Option<i64>,
    /// Write a recording that `replay` can check later.
    #[arg(long)]
    record: Option<PathBuf>,
}

fn engine_dir(args: &EngineArgs) -> PathBuf {
    args.engine_dir
        .clone()
        .or_else(|| std::env::var_os("RENETHACK_ENGINE_DIR").map(PathBuf::from))
        // a development checkout: <repo>/client/rust/nh-cli -> <repo>/engine/build
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/build"))
}

/// Keeps a temporary playground alive for the whole command.
struct Playground {
    path: PathBuf,
    _temp: Option<tempfile::TempDir>,
}

fn playground(args: &EngineArgs) -> Result<Playground, LinkError> {
    let (path, temp) = match &args.playground {
        Some(p) => (p.clone(), None),
        None => {
            let t = tempfile::tempdir()?;
            (t.path().to_path_buf(), Some(t))
        }
    };
    create_playground(&engine_dir(args).join("data"), &path)?;
    Ok(Playground { path, _temp: temp })
}

fn default_options() -> String {
    EngineConfig::character_options("Hero", "valkyrie", "human", "female", "neutral")
        .expect("the default character is valid")
}

fn run(args: RunArgs) -> Result<ExitCode, LinkError> {
    let script = std::fs::read_to_string(&args.script)?;
    let pg = playground(&args.engine)?;
    let seed = args.seed.unwrap_or_else(|| now() as u64);
    let fixed_time = args.fixed_time.unwrap_or_else(now);
    let cfg = EngineConfig {
        engine: engine_dir(&args.engine).join("nh-engine"),
        playground: pg.path.clone(),
        options: args.options.unwrap_or_else(default_options),
        seed: Some(seed),
        fixed_time: Some(fixed_time),
    };
    let mut engine = Engine::spawn(&cfg)?;
    let mut responder = ScriptResponder::new(parse_script(&script)?);
    let t = run_session(&mut engine, &mut responder, &SessionLimits::default())?;
    print_transcript(&t);
    if let Some(path) = args.record {
        let hello = t.hello.clone().expect("session had a hello");
        Recording {
            header: RecordingHeader {
                format: RECORDING_FORMAT,
                seed,
                fixed_time,
                options: cfg.options.clone(),
                engine: hello.engine,
                patchset: hello.patchset,
                stream_hash: t.stream_hash(),
            },
            replies: t.replies.clone(),
        }
        .save(&path)?;
        println!("recorded {} replies to {}", t.replies.len(), path.display());
    }
    Ok(exit_code(&t))
}

fn replay(engine_args: EngineArgs, path: &Path) -> Result<ExitCode, LinkError> {
    let rec = Recording::load(path)?;
    let pg = playground(&engine_args)?;
    let cfg = EngineConfig {
        engine: engine_dir(&engine_args).join("nh-engine"),
        playground: pg.path.clone(),
        options: rec.header.options.clone(),
        seed: Some(rec.header.seed),
        fixed_time: Some(rec.header.fixed_time),
    };
    let mut engine = Engine::spawn(&cfg)?;
    let t = run_session(
        &mut engine,
        &mut ReplayResponder::new(&rec),
        &SessionLimits::default(),
    )?;
    if t.stream_hash() == rec.header.stream_hash {
        println!("replay matches: {}", t.stream_hash());
        Ok(ExitCode::SUCCESS)
    } else {
        println!(
            "replay differs: recorded {}, got {}",
            rec.header.stream_hash,
            t.stream_hash()
        );
        Ok(ExitCode::FAILURE)
    }
}

fn catalog(engine_args: EngineArgs) -> Result<ExitCode, LinkError> {
    let pg = playground(&engine_args)?;
    let cfg = EngineConfig {
        engine: engine_dir(&engine_args).join("nh-engine"),
        playground: pg.path.clone(),
        options: default_options(),
        seed: None,
        fixed_time: None,
    };
    let mut engine = Engine::spawn(&cfg)?;
    while let Some(inc) = engine.recv(Duration::from_secs(10))? {
        match inc.msg {
            EngineMsg::Hello(h) => println!(
                "engine {} (protocol {}, patchset {})",
                h.engine, h.protocol, h.patchset
            ),
            EngineMsg::Catalog(c) => {
                println!(
                    "monsters {}, object tiles {}, map symbols {}, roles {}, races {}",
                    c.monsters.len(),
                    c.object_tiles.len(),
                    c.cmap.len(),
                    c.roles.len(),
                    c.races.len()
                );
                engine.kill();
                return Ok(ExitCode::SUCCESS);
            }
            _ => {}
        }
    }
    Err(LinkError::Handshake(
        "engine exited before sending the catalog".into(),
    ))
}

fn print_transcript(t: &Transcript) {
    let mut message_win = None;
    for line in &t.lines {
        match parse_line(line) {
            Ok(EngineMsg::Win(WinCall::CreateNhwindow {
                win,
                kind: WindowKind::Message,
            })) => message_win = Some(win),
            Ok(EngineMsg::Win(WinCall::Putstr { win, text, .. })) if Some(win) == message_win => {
                println!("| {text}")
            }
            Ok(EngineMsg::Win(WinCall::RawPrint { text, .. })) if !text.is_empty() => {
                println!("! {text}")
            }
            Ok(EngineMsg::Req {
                req: Request::YnFunction { query, .. },
                ..
            }) => println!("? {query}"),
            Ok(EngineMsg::Req {
                req: Request::Getlin { query },
                ..
            }) => println!("? {query}"),
            Ok(EngineMsg::Error { msg }) => println!("engine error: {msg}"),
            _ => {}
        }
    }
    println!(
        "exit {:?}, bye {}, {} lines, stream {}",
        t.exit.and_then(|s| s.code()),
        t.said_bye,
        t.lines.len(),
        t.stream_hash()
    );
}

fn exit_code(t: &Transcript) -> ExitCode {
    if t.exit.is_some_and(|s| s.success()) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Run(args) => run(args),
        Cmd::Replay { engine, recording } => replay(engine, &recording),
        Cmd::Catalog { engine } => catalog(engine),
    };
    result.unwrap_or_else(|e| {
        eprintln!("nh-cli: {e}");
        ExitCode::from(2)
    })
}
