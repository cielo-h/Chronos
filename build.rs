use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let local_ffmpeg = manifest_dir.join("lib").join("ffmpeg");

    let ffmpeg_dir = if local_ffmpeg.exists() {
        local_ffmpeg
    } else if let Ok(dir) = env::var("FFMPEG_DIR") {
        PathBuf::from(dir)
    } else {
        println!("cargo:warning=FFmpeg not found. Place it in lib/ffmpeg/ or set the FFMPEG_DIR environment variable.");
        return;
    };

    let lib_dir = ffmpeg_dir.join("lib");
    if lib_dir.exists() {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
    }

    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/lib");

    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/lib");

    println!("cargo:rustc-link-lib=dylib=avcodec");
    println!("cargo:rustc-link-lib=dylib=avformat");
    println!("cargo:rustc-link-lib=dylib=avutil");
    println!("cargo:rustc-link-lib=dylib=swscale");
    println!("cargo:rustc-link-lib=dylib=avfilter");

    #[cfg(target_os = "windows")]
    copy_dlls_to_target(&ffmpeg_dir);

    #[cfg(target_os = "windows")]
    {
        let mpv_dll = manifest_dir.join("lib").join("mpv").join("libmpv-2.dll");
        copy_dll_to_target(&mpv_dll);
    }

    println!("cargo:rerun-if-env-changed=FFMPEG_DIR");

    #[cfg(target_os = "windows")]
    {
        let bin_dir = ffmpeg_dir.join("bin");
        println!("cargo:rerun-if-changed={}", bin_dir.display());
    }
}

#[cfg(target_os = "windows")]
fn copy_dlls_to_target(ffmpeg_dir: &PathBuf) {
    let bin_dir = ffmpeg_dir.join("bin");
    if !bin_dir.exists() {
        println!(
            "cargo:warning=FFmpeg bin/ が見つかりません: {}",
            bin_dir.display()
        );
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = match out_dir.ancestors().nth(3) {
        Some(p) => p.to_path_buf(),
        None => {
            println!("cargo:warning=ターゲットディレクトリの特定に失敗しました");
            return;
        }
    };

    let entries = match std::fs::read_dir(&bin_dir) {
        Ok(e) => e,
        Err(err) => {
            println!(
                "cargo:warning=FFmpeg bin/ の読み取りに失敗しました: {}",
                err
            );
            return;
        }
    };

    for entry in entries.flatten() {
        let src = entry.path();
        if src.extension().and_then(|e| e.to_str()) == Some("dll") {
            let file_name = match src.file_name() {
                Some(n) => n,
                None => continue,
            };
            let dest = target_dir.join(file_name);

            if dest.exists() {
                if let (Ok(src_meta), Ok(dest_meta)) = (src.metadata(), dest.metadata()) {
                    if src_meta.len() == dest_meta.len() {
                        continue;
                    }
                }
            }

            match std::fs::copy(&src, &dest) {
                Ok(_) => println!(
                    "cargo:warning=DLLをコピーしました: {} → {}",
                    src.display(),
                    dest.display()
                ),
                Err(err) => println!(
                    "cargo:warning=DLLのコピーに失敗しました {}: {}",
                    file_name.to_string_lossy(),
                    err
                ),
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn copy_dll_to_target(dll_path: &PathBuf) {
    if !dll_path.exists() {
        println!(
            "cargo:warning=DLLが見つかりません: {}",
            dll_path.display()
        );
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = match out_dir.ancestors().nth(3) {
        Some(p) => p.to_path_buf(),
        None => {
            println!("cargo:warning=ターゲットディレクトリの特定に失敗しました");
            return;
        }
    };

    let file_name = match dll_path.file_name() {
        Some(n) => n,
        None => return,
    };
    let dest = target_dir.join(file_name);

    if dest.exists() {
        if let (Ok(src_meta), Ok(dest_meta)) = (dll_path.metadata(), dest.metadata()) {
            if src_meta.len() == dest_meta.len() {
                return;
            }
        }
    }

    match std::fs::copy(dll_path, &dest) {
        Ok(_) => println!(
            "cargo:warning=DLLをコピーしました: {} → {}",
            dll_path.display(),
            dest.display()
        ),
        Err(err) => println!(
            "cargo:warning=DLLのコピーに失敗しました {}: {}",
            file_name.to_string_lossy(),
            err
        ),
    }

    println!("cargo:rerun-if-changed={}", dll_path.display());
}
