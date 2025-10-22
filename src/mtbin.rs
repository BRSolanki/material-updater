use materialbin::{
    bgfx_shader::BgfxShader,
    pass::{ShaderStage, ShaderCodePlatform},
    CompiledMaterialDefinition, MinecraftVersion,
};
use memchr::memmem::Finder;
// use ndk::asset::{Asset, AssetManager};
// use ndk_sys::{AAsset, AAssetManager};
use scroll::Pread;
use std::{
    ffi::{CStr, CString, OsStr},
    io::{self, Cursor, Read, Seek, Write},
    // os::unix::ffi::OsStrExt,
    sync::{
        atomic::{AtomicBool, Ordering},
        LazyLock, Mutex, OnceLock,
    },
};

// The Minecraft version we will use to port shaders to
static MC_VERSION: OnceLock<Option<MinecraftVersion>> = OnceLock::new();
static IS_UPPER_1_21_100: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "autofix")]
pub(crate) fn process_material(man: *mut AAssetManager, data: &[u8]) -> Option<Vec<u8>> {
    let mcver = MC_VERSION.get_or_init(|| {
        let pointer = match std::ptr::NonNull::new(man) {
            Some(yay) => yay,
            None => {
                log::warn!("AssetManager is null?, preposterous, mc detection failed");
                return None;
            }
        };
        let manager = unsafe { ndk::asset::AssetManager::from_ptr(pointer) };
        get_current_mcver(manager)
    });
    // Just ignore if no Minecraft version was found
    let mcver = (*mcver)?;
    for version in materialbin::ALL_VERSIONS {
        let mut material: CompiledMaterialDefinition = match data.pread_with(0, version) {
            Ok(data) => data,
            Err(e) => {
                log::trace!("[version] Parsing failed: {e}");
                continue;
            }
        };
        // Prevent some work
        if version == mcver 
        {
            // return None;
        }
        if (material.name == "RenderChunk" || material.name == "RenderChunkPrepass")
            && version != MinecraftVersion::V1_21_110
            && IS_UPPER_1_21_100.load(Ordering::Acquire)
        {
            handle_lightmaps(&mut material, version);
        }
        if material.name == "RenderChunk" && (
            mcver == MinecraftVersion::V1_20_80 ||
            mcver == MinecraftVersion::V1_21_20 ||
            mcver == MinecraftVersion::V1_21_110
        ) && (
            version == MinecraftVersion::V1_19_60 ||
            version == MinecraftVersion::V1_18_30
        ) {
            handle_samplers(&mut material);
        }
        let mut output = Vec::with_capacity(data.len());
        if let Err(e) = material.write(&mut output, mcver) {
            log::trace!("[version] Write error: {e}");
            return None;
        }
        return Some(output);
    }
    None
}

pub(crate) fn handle_lightmaps(materialbin: &mut CompiledMaterialDefinition) {
    log::info!("mtbinloader25 handle_lightmaps");
    let pattern = b"void main";
    let replace_with = b"
#define a_texcoord1 vec2(fract(a_texcoord1.x*15.9375)+0.0001,floor(a_texcoord1.x*15.9375)*0.0625+0.0001)
void main";
    let finder = Finder::new(pattern);
    let finder1 = Finder::new(b"v_lightmapUV = a_texcoord1;");
    let finder2 = Finder::new(b"v_lightmapUV=a_texcoord1;");
    let finder3 = Finder::new(b"#define a_texcoord1 ");
    for (_, pass) in &mut materialbin.passes {
        for variants in &mut pass.variants {
            for (stage, code) in &mut variants.shader_codes {
                if stage.stage == ShaderStage::Vertex {
                    // log::info!("mtbinloader25 handle_lightmaps");
                    let mut bgfx: BgfxShader = code.bgfx_shader_data.pread(0).unwrap();
                    // if version == MinecraftVersion::V1_21_20 
                    // if stage.platform == ShaderCodePlatform::Essl100 
                    if (
                      finder3.find(&bgfx.code).is_some() || (
                      finder1.find(&bgfx.code).is_none() &&
                      finder2.find(&bgfx.code).is_none() )) {
                        log::warn!("Skipping replacement due to not existing lightmap UV assignment");
                        continue;
                    }; 
                    log::info!("autofix is doing lightmap replacing...");
                    replace_bytes(&mut bgfx.code, &finder, pattern, replace_with);
                    code.bgfx_shader_data.clear();
                    bgfx.write(&mut code.bgfx_shader_data).unwrap();
                }
            }
        }
    }
}
fn handle_samplers(materialbin: &mut CompiledMaterialDefinition) {
    log::info!("mtbinloader25 handle_samplers");
    let pattern = b"void main ()";
    let replace_with = b"
#if __VERSION__ >= 300
 #define texture(tex,uv) textureLod(tex,uv,0.0)
#else
 #define texture2D(tex,uv) texture2DLod(tex,uv,0.0)
#endif
void main ()";
    let finder = Finder::new(pattern);
    for (_passes, pass) in &mut materialbin.passes {
        if _passes == "AlphaTest" || _passes == "Opaque" {
            for variants in &mut pass.variants {
                for (stage, code) in &mut variants.shader_codes {
                    if stage.stage == ShaderStage::Fragment && stage.platform_name == "ESSL_100" {
                        // log::info!("mtbinloader25 handle_samplers");
                        let mut bgfx: BgfxShader = code.bgfx_shader_data.pread(0).unwrap();
                        replace_bytes(&mut bgfx.code, &finder, pattern, replace_with);
                        code.bgfx_shader_data.clear();
                        bgfx.write(&mut code.bgfx_shader_data).unwrap();
                    }
                }
            }
        }
    }
}
fn replace_bytes(codebuf: &mut Vec<u8>, finder: &Finder, pattern: &[u8], replace_with: &[u8]) {
    let sus = match finder.find(codebuf) {
        Some(yay) => yay,
        None => {
            println!("oops");
            return;
        }
    };
    codebuf.splice(sus..sus + pattern.len(), replace_with.iter().cloned());
}
