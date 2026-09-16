use radix_wasm_instrument::{
    gas_metering::{self, mutable_global, ConstantCostRules},
    inject_stack_limiter,
    utils::module_info::ModuleInfo,
};
use std::fmt;
use wasm_encoder::SectionId;
use wasmparser::{ExternalKind, Operator, Payload, ValType, Validator, WasmFeatures};

pub const PIPELINE_VERSION: u32 = 1;
pub const GAS_GLOBAL_EXPORT: &str = "gas_left";
pub const REQUIRED_FUNC_EXPORTS: [&str; 6] = [
    "abi_version",
    "alloc",
    "dealloc",
    "manifest",
    "decide",
    "view",
];
pub const ENGINE_FUNC_EXPORTS: [&str; 9] = [
    "abi_version",
    "alloc",
    "dealloc",
    "fold_entry",
    "fold_log",
    "decide_request",
    "snapshot",
    "restore",
    "view",
];
pub const MEMORY_EXPORT: &str = "memory";
pub const DEFAULT_GAS_LIMIT: u64 = 100_000_000;
pub const DEFAULT_STACK_HEIGHT_LIMIT: u32 = 512;
pub const DEFAULT_MEMORY_MAX_PAGES: u64 = 256;
pub const ENGINE_GAS_LIMIT: u64 = 10_000_000_000;
pub const ENGINE_MEMORY_MAX_PAGES: u64 = 4096;

#[derive(Clone, Debug)]
pub struct HardenConfig {
    pub gas_limit: u64,
    pub stack_height_limit: u32,
    pub memory_max_pages: u64,
    pub required_exports: Vec<String>,
    pub allow_floats: bool,
}

impl Default for HardenConfig {
    fn default() -> Self {
        Self {
            gas_limit: DEFAULT_GAS_LIMIT,
            stack_height_limit: DEFAULT_STACK_HEIGHT_LIMIT,
            memory_max_pages: DEFAULT_MEMORY_MAX_PAGES,
            required_exports: REQUIRED_FUNC_EXPORTS
                .iter()
                .map(|name| name.to_string())
                .collect(),
            allow_floats: false,
        }
    }
}

impl HardenConfig {
    pub fn engine() -> Self {
        Self {
            gas_limit: ENGINE_GAS_LIMIT,
            memory_max_pages: ENGINE_MEMORY_MAX_PAGES,
            required_exports: ENGINE_FUNC_EXPORTS
                .iter()
                .map(|name| name.to_string())
                .collect(),
            allow_floats: true,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug)]
pub struct HardenReport {
    pub pipeline_version: u32,
    pub gas_limit: u64,
    pub stack_height_limit: u32,
    pub memory_max_pages: u64,
    pub input_len: usize,
    pub output_len: usize,
    pub input_hash: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct HardenedModule {
    pub bytes: Vec<u8>,
    pub hash: [u8; 32],
    pub report: HardenReport,
}

impl HardenedModule {
    pub fn hash_hex(&self) -> String {
        blake3::Hash::from_bytes(self.hash).to_hex().to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HardenError {
    Parse(String),
    ImportForbidden { module: String, name: String },
    ForbiddenOpcode { func: u32, opcode: String },
    FloatType { context: String },
    MissingExport { name: String },
    ReservedExport { name: String },
    NoMemory,
    MultipleMemories { count: u32 },
    SharedMemory,
    Memory64,
    MemoryMinAboveCap { min_pages: u64, cap_pages: u64 },
    UnsupportedGlobal { index: u32 },
    Instrument(String),
    Reencode(String),
    OutputInvalid(String),
}

impl fmt::Display for HardenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "module does not parse: {e}"),
            Self::ImportForbidden { module, name } => {
                write!(f, "imports are forbidden, found {module}::{name}")
            }
            Self::ForbiddenOpcode { func, opcode } => {
                write!(f, "forbidden float/simd opcode {opcode} in function {func}")
            }
            Self::FloatType { context } => {
                write!(f, "forbidden float/simd value type in {context}")
            }
            Self::MissingExport { name } => write!(f, "missing required export {name}"),
            Self::ReservedExport { name } => {
                write!(f, "export name {name} is reserved for the pipeline")
            }
            Self::NoMemory => write!(f, "module declares no memory"),
            Self::MultipleMemories { count } => {
                write!(f, "module declares {count} memories, expected exactly 1")
            }
            Self::SharedMemory => write!(f, "shared memories are forbidden"),
            Self::Memory64 => write!(f, "64-bit memories are forbidden"),
            Self::MemoryMinAboveCap {
                min_pages,
                cap_pages,
            } => {
                write!(
                    f,
                    "memory minimum of {min_pages} pages exceeds the cap of {cap_pages} pages"
                )
            }
            Self::UnsupportedGlobal { index } => {
                write!(f, "global {index} has an unsupported type or initializer")
            }
            Self::Instrument(e) => write!(f, "instrumentation failed: {e}"),
            Self::Reencode(e) => write!(f, "re-encoding failed: {e}"),
            Self::OutputInvalid(e) => write!(f, "hardened module failed validation: {e}"),
        }
    }
}

impl std::error::Error for HardenError {}

pub fn harden(module: &[u8], config: &HardenConfig) -> Result<HardenedModule, HardenError> {
    scan_input(module, config)?;
    validate_features(module, config)?;

    let clamped = strip_customs_and_clamp_memory(module, config.memory_max_pages)?;
    let stack_limited = {
        let mut info = parse_info(&clamped)?;
        inject_stack_limiter(&mut info, config.stack_height_limit)
            .map_err(|e| HardenError::Instrument(e.to_string()))?
    };
    let metered = {
        let mut info = parse_info(&stack_limited)?;
        gas_metering::inject(
            &mut info,
            mutable_global::Injector::new("env", GAS_GLOBAL_EXPORT),
            &ConstantCostRules::default(),
        )
        .map_err(|e| HardenError::Instrument(e.to_string()))?
    };
    let bytes = bake_gas_limit(&metered, config.gas_limit)?;

    validate_features(&bytes, config).map_err(|e| HardenError::OutputInvalid(e.to_string()))?;
    scan_output(&bytes)?;

    let hash = *blake3::hash(&bytes).as_bytes();
    let report = HardenReport {
        pipeline_version: PIPELINE_VERSION,
        gas_limit: config.gas_limit,
        stack_height_limit: config.stack_height_limit,
        memory_max_pages: config.memory_max_pages,
        input_len: module.len(),
        output_len: bytes.len(),
        input_hash: *blake3::hash(module).as_bytes(),
    };
    Ok(HardenedModule {
        bytes,
        hash,
        report,
    })
}

fn parse_info(bytes: &[u8]) -> Result<ModuleInfo, HardenError> {
    ModuleInfo::new(bytes).map_err(|e| HardenError::Parse(e.to_string()))
}

fn features(config: &HardenConfig) -> WasmFeatures {
    WasmFeatures {
        mutable_global: true,
        saturating_float_to_int: config.allow_floats,
        sign_extension: true,
        reference_types: true,
        multi_value: true,
        bulk_memory: true,
        simd: false,
        relaxed_simd: false,
        threads: false,
        tail_call: false,
        floats: config.allow_floats,
        multi_memory: false,
        exceptions: false,
        memory64: false,
        extended_const: false,
        component_model: false,
        function_references: false,
        memory_control: false,
        gc: false,
    }
}

fn validate_features(bytes: &[u8], config: &HardenConfig) -> Result<(), HardenError> {
    Validator::new_with_features(features(config))
        .validate_all(bytes)
        .map(|_| ())
        .map_err(|e| HardenError::Parse(e.to_string()))
}

fn val_type_forbidden(ty: &ValType) -> bool {
    matches!(ty, ValType::F32 | ValType::F64 | ValType::V128)
}

fn opcode_name(op: &Operator) -> String {
    let debug = format!("{op:?}");
    debug.split([' ', '{']).next().unwrap_or(&debug).to_string()
}

fn opcode_forbidden(name: &str) -> bool {
    name.contains("F32")
        || name.contains("F64")
        || name.contains("V128")
        || name.contains("8x16")
        || name.contains("16x8")
        || name.contains("32x4")
        || name.contains("64x2")
}

fn scan_input(bytes: &[u8], config: &HardenConfig) -> Result<(), HardenError> {
    let mut func_exports = Vec::new();
    let mut memory_export = false;
    let mut memory_count = 0u32;
    let mut func_index = 0u32;

    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| HardenError::Parse(e.to_string()))? {
            Payload::TypeSection(reader) => {
                for ty in reader {
                    match ty.map_err(|e| HardenError::Parse(e.to_string()))? {
                        wasmparser::Type::Func(func) => {
                            if !config.allow_floats
                                && (func.params().iter().any(val_type_forbidden)
                                    || func.results().iter().any(val_type_forbidden))
                            {
                                return Err(HardenError::FloatType {
                                    context: "function signature".to_string(),
                                });
                            }
                        }
                        wasmparser::Type::Array(_) => {
                            return Err(HardenError::Parse(
                                "array types are not supported".to_string(),
                            ));
                        }
                    }
                }
            }
            Payload::ImportSection(reader) => {
                if let Some(import) = reader.into_iter().next() {
                    let import = import.map_err(|e| HardenError::Parse(e.to_string()))?;
                    return Err(HardenError::ImportForbidden {
                        module: import.module.to_string(),
                        name: import.name.to_string(),
                    });
                }
            }
            Payload::GlobalSection(reader) => {
                for global in reader {
                    let global = global.map_err(|e| HardenError::Parse(e.to_string()))?;
                    if !config.allow_floats && val_type_forbidden(&global.ty.content_type) {
                        return Err(HardenError::FloatType {
                            context: "global".to_string(),
                        });
                    }
                }
            }
            Payload::MemorySection(reader) => {
                for memory in reader {
                    let memory = memory.map_err(|e| HardenError::Parse(e.to_string()))?;
                    memory_count += 1;
                    if memory.shared {
                        return Err(HardenError::SharedMemory);
                    }
                    if memory.memory64 {
                        return Err(HardenError::Memory64);
                    }
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.map_err(|e| HardenError::Parse(e.to_string()))?;
                    if export.name == GAS_GLOBAL_EXPORT {
                        return Err(HardenError::ReservedExport {
                            name: export.name.to_string(),
                        });
                    }
                    match export.kind {
                        ExternalKind::Func => func_exports.push(export.name.to_string()),
                        ExternalKind::Memory if export.name == MEMORY_EXPORT => {
                            memory_export = true;
                        }
                        _ => {}
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                if !config.allow_floats {
                    let locals = body
                        .get_locals_reader()
                        .map_err(|e| HardenError::Parse(e.to_string()))?;
                    for local in locals {
                        let (_, ty) = local.map_err(|e| HardenError::Parse(e.to_string()))?;
                        if val_type_forbidden(&ty) {
                            return Err(HardenError::FloatType {
                                context: format!("locals of function {func_index}"),
                            });
                        }
                    }
                    let ops = body
                        .get_operators_reader()
                        .map_err(|e| HardenError::Parse(e.to_string()))?;
                    for op in ops {
                        let op = op.map_err(|e| HardenError::Parse(e.to_string()))?;
                        let name = opcode_name(&op);
                        if opcode_forbidden(&name) {
                            return Err(HardenError::ForbiddenOpcode {
                                func: func_index,
                                opcode: name,
                            });
                        }
                    }
                }
                func_index += 1;
            }
            _ => {}
        }
    }

    for required in &config.required_exports {
        if !func_exports.iter().any(|name| name == required) {
            return Err(HardenError::MissingExport {
                name: required.to_string(),
            });
        }
    }
    if memory_count == 0 {
        return Err(HardenError::NoMemory);
    }
    if memory_count > 1 {
        return Err(HardenError::MultipleMemories {
            count: memory_count,
        });
    }
    if !memory_export {
        return Err(HardenError::MissingExport {
            name: MEMORY_EXPORT.to_string(),
        });
    }
    Ok(())
}

fn scan_output(bytes: &[u8]) -> Result<(), HardenError> {
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let Payload::ImportSection(reader) =
            payload.map_err(|e| HardenError::Parse(e.to_string()))?
        {
            if let Some(import) = reader.into_iter().next() {
                let import = import.map_err(|e| HardenError::Parse(e.to_string()))?;
                return Err(HardenError::ImportForbidden {
                    module: import.module.to_string(),
                    name: import.name.to_string(),
                });
            }
        }
    }
    Ok(())
}

fn strip_customs_and_clamp_memory(bytes: &[u8], cap_pages: u64) -> Result<Vec<u8>, HardenError> {
    let mut info = parse_info(bytes)?;
    info.raw_sections.remove(&u8::from(SectionId::Custom));
    let mut section = wasm_encoder::MemorySection::new();
    for memory in &info.memory_types {
        if memory.initial > cap_pages {
            return Err(HardenError::MemoryMinAboveCap {
                min_pages: memory.initial,
                cap_pages,
            });
        }
        let maximum = memory.maximum.map_or(cap_pages, |max| max.min(cap_pages));
        section.memory(wasm_encoder::MemoryType {
            minimum: memory.initial,
            maximum: Some(maximum),
            memory64: false,
            shared: false,
        });
    }
    info.replace_section(SectionId::Memory.into(), &section)
        .map_err(|e| HardenError::Reencode(e.to_string()))?;
    Ok(info.bytes())
}

fn bake_gas_limit(bytes: &[u8], gas_limit: u64) -> Result<Vec<u8>, HardenError> {
    let mut info = parse_info(bytes)?;
    let exports = info
        .export_section()
        .map_err(|e| HardenError::Reencode(e.to_string()))?
        .unwrap_or_default();
    let gas_global_index = exports
        .iter()
        .find(|export| {
            export.name == GAS_GLOBAL_EXPORT && matches!(export.kind, ExternalKind::Global)
        })
        .map(|export| export.index)
        .ok_or_else(|| HardenError::MissingExport {
            name: GAS_GLOBAL_EXPORT.to_string(),
        })?;
    let globals = info
        .global_section()
        .map_err(|e| HardenError::Reencode(e.to_string()))?
        .unwrap_or_default();

    let mut section = wasm_encoder::GlobalSection::new();
    for (index, global) in globals.iter().enumerate() {
        let init = if index as u32 == gas_global_index {
            wasm_encoder::ConstExpr::i64_const(gas_limit as i64)
        } else {
            translate_init(global).ok_or(HardenError::UnsupportedGlobal {
                index: index as u32,
            })?
        };
        let ty = match global.ty.content_type {
            ValType::I32 => wasm_encoder::ValType::I32,
            ValType::I64 => wasm_encoder::ValType::I64,
            _ => {
                return Err(HardenError::UnsupportedGlobal {
                    index: index as u32,
                })
            }
        };
        section.global(
            wasm_encoder::GlobalType {
                val_type: ty,
                mutable: global.ty.mutable,
            },
            &init,
        );
    }
    info.replace_section(SectionId::Global.into(), &section)
        .map_err(|e| HardenError::Reencode(e.to_string()))?;
    Ok(info.bytes())
}

fn translate_init(global: &wasmparser::Global) -> Option<wasm_encoder::ConstExpr> {
    let mut reader = global.init_expr.get_operators_reader();
    let op = reader.read().ok()?;
    let init = match op {
        Operator::I32Const { value } => wasm_encoder::ConstExpr::i32_const(value),
        Operator::I64Const { value } => wasm_encoder::ConstExpr::i64_const(value),
        _ => return None,
    };
    match reader.read().ok()? {
        Operator::End => Some(init),
        _ => None,
    }
}
