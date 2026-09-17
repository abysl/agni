use std::error::Error;
use std::path::PathBuf;

use agni_importers::riftbound::gateway::{deck_resolver, RESOLVER_NAME};
use spirit_node::spirit_core::record::Tdr;
use spirit_node::spirit_core::{identity, BlobHash, BlobStore, Trust, TrustLevel};
use spirit_node::{gateway, serve_mesh};
use spirit_schema::modules::{self, Module, Role};

struct PublishedModule {
    name: String,
    role: Role,
    version: String,
    path: PathBuf,
}

fn parse_publish(spec: &str) -> Result<PublishedModule, String> {
    let mut parts = spec.splitn(4, ':');
    let name = parts.next().filter(|s| !s.is_empty());
    let role = parts.next().and_then(|role| match role {
        "engine" => Some(Role::Engine),
        "plugin" => Some(Role::Plugin),
        _ => None,
    });
    let version = parts.next().filter(|s| !s.is_empty());
    let path = parts.next().filter(|s| !s.is_empty());
    match (name, role, version, path) {
        (Some(name), Some(role), Some(version), Some(path)) => Ok(PublishedModule {
            name: name.to_string(),
            role,
            version: version.to_string(),
            path: PathBuf::from(path),
        }),
        _ => Err(format!(
            "--publish-module wants name:engine|plugin:version:path, got {spec:?}"
        )),
    }
}

fn publish_modules(dir: &PathBuf, wanted: &[PublishedModule]) -> Result<(), String> {
    if wanted.is_empty() {
        return Ok(());
    }
    let store = BlobStore::open(dir).map_err(|e| e.to_string())?;
    let identity = identity::load_or_create(dir).map_err(|e| e.to_string())?;
    let trust = Trust::load(dir).with_own(identity.dgid());
    let td =
        Tdr::new("deck-gateway", &("nix", env!("CARGO_PKG_VERSION"))).map_err(|e| e.to_string())?;
    for module in wanted {
        let bytes = std::fs::read(&module.path)
            .map_err(|e| format!("modules/{}: {}: {e}", module.name, module.path.display()))?;
        let declared = Module::new(
            &module.name,
            module.role,
            &module.version,
            agni_sim::abi::ENGINE_ABI_VERSION,
        );
        let ci = declared.ci().map_err(|e| e.to_string())?;
        let blob = BlobHash::of(&bytes);
        let already = modules::versions(&store, &module.name).iter().any(|held| {
            held.ci == ci
                && held.blob == Some(blob)
                && held.held
                && held.trusted(&trust, TrustLevel::Cache)
        });
        if already {
            println!(
                "modules/{} {} @ {blob} already published",
                module.name, module.version
            );
            continue;
        }
        modules::publish(&store, &identity, &declared, &td, &bytes)?;
        println!(
            "published modules/{} {} @ {blob}",
            module.name, module.version
        );
    }
    Ok(())
}

fn env_list(key: &str) -> Vec<String> {
    std::env::var(key)
        .ok()
        .map(|raw| {
            raw.split(|c: char| c.is_whitespace() || c == ',')
                .filter(|item| !item.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn seeds_from(path: Option<String>) -> Vec<String> {
    let Some(path) = path else {
        return Vec::new();
    };
    std::fs::read_to_string(path)
        .map(|raw| {
            raw.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut dir: Option<PathBuf> = None;
    let mut seed_file: Option<String> = None;
    let mut wants: Vec<String> = Vec::new();
    let mut port: Option<u16> = None;
    let mut publish: Vec<PublishedModule> = Vec::new();
    let mut published_only = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--published-only" => published_only = true,
            "--seed-file" => seed_file = args.next(),
            "--publish-module" => match args.next() {
                Some(spec) => publish.push(parse_publish(&spec)?),
                None => return Err("--publish-module wants a value".into()),
            },
            "--want" => wants.extend(args.next()),
            "--gateway" => port = args.next().and_then(|value| value.parse().ok()),
            other => {
                if dir.is_none() {
                    dir = Some(PathBuf::from(other));
                }
            }
        }
    }
    let dir = dir.unwrap_or_else(|| {
        std::env::var("SPIRIT_STORE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    });
    let port = port.or_else(|| std::env::var("SPIRIT_GATEWAY").ok()?.parse().ok());
    let mut seeds = seeds_from(seed_file);
    seeds.extend(env_list("SPIRIT_SEEDS"));
    if wants.is_empty() {
        wants = env_list("SPIRIT_WANTS");
    }

    publish_modules(&dir, &publish)?;
    match agni_importers::asset_gateway::publish_index(&dir) {
        Ok(hash) => println!("assets index: {hash}"),
        Err(error) => eprintln!("assets index not published: {error}"),
    }
    let serving = if published_only {
        spirit_node::serve_published(&dir, &seeds).await?
    } else {
        serve_mesh(&dir, &seeds, &wants).await?
    };
    if let Some(port) = port {
        let bound = gateway::spawn(
            port,
            gateway::Gateway {
                dir: dir.clone(),
                node_id: serving.node_id.clone(),
                mesh: serving.mesh.clone(),
                resolvers: gateway::Resolvers::from([
                    (RESOLVER_NAME.to_string(), deck_resolver(dir.clone())),
                    (
                        agni_importers::asset_gateway::RESOLVER_NAME.to_string(),
                        if published_only {
                            agni_importers::asset_gateway::published_asset_resolver(dir.clone())
                        } else {
                            agni_importers::asset_gateway::asset_resolver(
                                dir.clone(),
                                serving.mesh.clone(),
                            )
                        },
                    ),
                ]),
            },
        )
        .await?;
        println!("gateway: http://127.0.0.1:{bound}/gateway/status");
    }
    println!("node: {}", serving.node_id);
    println!("identity: {}", serving.ticket);
    for served in &serving.refs {
        println!("ref {}: {}", served.name, served.ticket);
    }
    println!(
        "deck gateway up — {} seed(s), {} want(s)",
        seeds.len(),
        wants.len()
    );
    tokio::signal::ctrl_c().await?;
    serving.shutdown().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("deck-gateway-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_publish_spec_names_the_module_its_role_version_and_file() {
        let parsed = parse_publish("riftbound:plugin:0.5.0:/nix/store/x/riftbound.wasm").unwrap();
        assert_eq!(parsed.name, "riftbound");
        assert_eq!(parsed.role, Role::Plugin);
        assert_eq!(parsed.version, "0.5.0");
        assert_eq!(parsed.path, PathBuf::from("/nix/store/x/riftbound.wasm"));
        assert!(parse_publish("engine:kernel:1:/x").is_err());
        assert!(parse_publish("engine:engine:1").is_err());
    }

    #[test]
    fn publishing_the_same_bytes_twice_keeps_one_held_trusted_version() {
        let dir = scratch("publish");
        let wasm = dir.join("riftbound.wasm");
        std::fs::write(&wasm, b"the plugin").unwrap();
        let wanted = vec![PublishedModule {
            name: "riftbound".into(),
            role: Role::Plugin,
            version: "0.5.0".into(),
            path: wasm,
        }];
        publish_modules(&dir, &wanted).unwrap();
        publish_modules(&dir, &wanted).unwrap();
        let store = BlobStore::open(&dir).unwrap();
        let identity = identity::load_or_create(&dir).unwrap();
        let trust = Trust::load(&dir).with_own(identity.dgid());
        let held = modules::versions(&store, "riftbound");
        assert_eq!(held.len(), 1);
        assert!(held[0].held);
        assert!(!held[0].legacy);
        assert!(held[0].trusted(&trust, TrustLevel::Cache));
        assert_eq!(held[0].module.version, "0.5.0");
        assert_eq!(
            held[0].module.abi_version,
            agni_sim::abi::ENGINE_ABI_VERSION
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
