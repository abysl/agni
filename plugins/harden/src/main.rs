use agni_harden::{harden, HardenConfig};
use serde::Serialize;
use spirit_sdk::modules::{self, Module, Role};
use spirit_sdk::record::Tdr;
use spirit_sdk::{identity, BlobHash, BlobStore};
use std::process::ExitCode;

const USAGE: &str = "usage: agni-harden <input.wasm> <output.wasm> [--engine] [--gas-limit N] [--stack-height-limit N] [--memory-max-pages N] [--store DIR --name NAME [--module-version VERSION] [--abi-version N]]";

struct PublishArgs {
    store: Option<String>,
    name: Option<String>,
    version: String,
    abi_version: u32,
    engine: bool,
}

fn parse_args() -> Result<(String, String, HardenConfig, PublishArgs), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let engine = raw.iter().any(|arg| arg == "--engine");
    let mut config = if engine {
        HardenConfig::engine()
    } else {
        HardenConfig::default()
    };
    let mut publish = PublishArgs {
        store: None,
        name: None,
        version: "0.0.0".into(),
        abi_version: 0,
        engine,
    };
    let mut args = raw.into_iter();
    let mut positional = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--engine" => {}
            "--gas-limit" => {
                let value = args.next().ok_or("--gas-limit needs a value")?;
                config.gas_limit = value.parse().map_err(|_| "--gas-limit must be a u64")?;
            }
            "--stack-height-limit" => {
                let value = args.next().ok_or("--stack-height-limit needs a value")?;
                config.stack_height_limit = value
                    .parse()
                    .map_err(|_| "--stack-height-limit must be a u32")?;
            }
            "--memory-max-pages" => {
                let value = args.next().ok_or("--memory-max-pages needs a value")?;
                config.memory_max_pages = value
                    .parse()
                    .map_err(|_| "--memory-max-pages must be a u64")?;
            }
            "--store" => {
                publish.store = Some(args.next().ok_or("--store needs a directory")?);
            }
            "--name" => {
                publish.name = Some(args.next().ok_or("--name needs a value")?);
            }
            "--module-version" => {
                publish.version = args.next().ok_or("--module-version needs a value")?;
            }
            "--abi-version" => {
                let value = args.next().ok_or("--abi-version needs a value")?;
                publish.abi_version = value.parse().map_err(|_| "--abi-version must be a u32")?;
            }
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}")),
            _ => positional.push(arg),
        }
    }
    if publish.store.is_some() && publish.name.is_none() {
        return Err("--store needs --name".into());
    }
    match <[String; 2]>::try_from(positional) {
        Ok([input, output]) => Ok((input, output, config, publish)),
        Err(_) => Err(USAGE.to_string()),
    }
}

#[derive(Serialize)]
struct HardenTd {
    allow_floats: bool,
    gas_limit: u64,
    input: BlobHash,
    memory_max_pages: u64,
    pipeline_version: u32,
    required_exports: Vec<String>,
    stack_height_limit: u32,
    tool: String,
}

fn publish(
    publish: &PublishArgs,
    config: &HardenConfig,
    input: &[u8],
    pipeline_version: u32,
    bytes: &[u8],
) -> Result<(), String> {
    let Some(dir) = &publish.store else {
        return Ok(());
    };
    let name = publish.name.clone().expect("--store implies --name");
    let store = BlobStore::open(dir).map_err(|e| format!("opening store {dir}: {e}"))?;
    let identity = identity::load_or_create(std::path::Path::new(dir))
        .map_err(|e| format!("identity for store {dir}: {e}"))?;
    let source = store.put(input).map_err(|e| e.to_string())?;
    let td = Tdr::new(
        "wasm-harden",
        &HardenTd {
            allow_floats: config.allow_floats,
            gas_limit: config.gas_limit,
            input: source,
            memory_max_pages: config.memory_max_pages,
            pipeline_version,
            required_exports: config.required_exports.clone(),
            stack_height_limit: config.stack_height_limit,
            tool: format!("agni-harden {}", env!("CARGO_PKG_VERSION")),
        },
    )
    .map_err(|e| e.to_string())?;
    let role = if publish.engine {
        Role::Engine
    } else {
        Role::Plugin
    };
    let module = Module::new(&name, role, &publish.version, publish.abi_version);
    let published = modules::publish(&store, &identity, &module, &td, bytes)?;
    eprintln!(
        "published {} module {name} {} as {} into refs/modules/{name} (signed {})",
        role.label(),
        publish.version,
        published.ci,
        identity.dgid().short()
    );
    Ok(())
}

fn run() -> Result<String, String> {
    let (input, output, config, publish_args) = parse_args()?;
    let module = std::fs::read(&input).map_err(|e| format!("reading {input}: {e}"))?;
    let hardened = harden(&module, &config).map_err(|e| e.to_string())?;
    std::fs::write(&output, &hardened.bytes).map_err(|e| format!("writing {output}: {e}"))?;
    let report = &hardened.report;
    eprintln!(
        "pipeline v{}: {} bytes -> {} bytes, gas limit {}, stack height limit {}, memory max {} pages",
        report.pipeline_version,
        report.input_len,
        report.output_len,
        report.gas_limit,
        report.stack_height_limit,
        report.memory_max_pages
    );
    publish(
        &publish_args,
        &config,
        &module,
        report.pipeline_version,
        &hardened.bytes,
    )?;
    Ok(hardened.hash_hex())
}

fn main() -> ExitCode {
    match run() {
        Ok(hash) => {
            println!("{hash}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
