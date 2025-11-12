use bzip2::bufread::BzDecoder;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, BufReader, IsTerminal, Write},
    path::{Path, PathBuf},
};

use crate::{archive_json_path, Error, OsAndArch, Result};

pub struct TTCefVersion {
    pub file: CefFile,
    pub url: String,
    pub os_and_arch: OsAndArch,
    pub target: String,
}

impl TTCefVersion {
    pub fn download_archive<P>(&self, location: P, show_progress: bool) -> Result<PathBuf>
    where
        P: AsRef<Path>,
    {
        fs::create_dir_all(&location)?;
        let download_file = location.as_ref().join(self.file.name.clone());

        if download_file.exists() {
            if show_progress {
                println!("Verified archive: {}", download_file.display());
            }
            return Ok(download_file);
        }

        if show_progress {
            println!("Using archive url: {}", self.url);
        }

        let mut file = File::create(&download_file)?;

        let resp = ureq::get(&self.url).call()?;
        let expected = resp
            .headers()
            .get("Content-Length")
            .ok_or(Error::MissingContentLength)?;
        let expected = expected.to_str()?;
        let expected = expected
            .parse::<u64>()
            .map_err(|_| Error::InvalidContentLength(expected.to_owned()))?;

        let downloaded = if show_progress && io::stdout().is_terminal() {
            const DOWNLOAD_TEMPLATE: &str = "{msg} {spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})";

            let bar = indicatif::ProgressBar::new(expected);
            bar.set_style(
                indicatif::ProgressStyle::with_template(DOWNLOAD_TEMPLATE)
                    .expect("invalid template")
                    .progress_chars("##-"),
            );
            bar.set_message("Downloading");
            std::io::copy(
                &mut bar.wrap_read(resp.into_body().into_reader()),
                &mut file,
            )
        } else {
            let mut reader = resp.into_body().into_reader();
            std::io::copy(&mut reader, &mut file)
        }?;

        if downloaded != expected {
            return Err(Error::UnexpectedFileSize {
                downloaded,
                expected,
            });
        }

        if show_progress {
            println!("Downloaded archive: {}", download_file.display());
        }
        Ok(download_file)
    }

    pub fn write_archive_json<P>(&self, location: P) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let archive_version = serde_json::to_string_pretty(&self.file)?;
        let mut archive_json = File::create(archive_json_path(location))?;
        archive_json.write_all(archive_version.as_bytes())?;
        Ok(())
    }

    pub fn extract_target_archive<P, Q>(
        &self,
        archive: P,
        location: Q,
        show_progress: bool,
    ) -> Result<PathBuf>
    where
        P: AsRef<Path>,
        Q: AsRef<Path>,
    {
        if show_progress {
            println!("Extracting archive: {}", archive.as_ref().display());
        }
        let decoder = BzDecoder::new(BufReader::new(File::open(&archive)?));
        tar::Archive::new(decoder).unpack(&location)?;

        let mut extracted_dir = PathBuf::from(archive.as_ref().display().to_string());

        extracted_dir.set_file_name(format!(
            "Release_GN_{}",
            map_arch_to_ttcef(self.os_and_arch.arch)
        ));

        let cef_dir = self.os_and_arch.to_string();
        let cef_dir: PathBuf = location.as_ref().join(cef_dir);

        if cef_dir.exists() {
            let old_dir = location.as_ref().join(format!(
                "old_{}_{}",
                self.os_and_arch.os, self.os_and_arch.arch
            ));
            if show_progress {
                println!("Cleaning up: {}", old_dir.display());
            }
            fs::rename(&cef_dir, &old_dir)?;
            fs::remove_dir_all(old_dir)?;
        }
        const RELEASE_DIR: &str = "Release";
        fs::rename(extracted_dir.join(RELEASE_DIR), &cef_dir)?;

        if self.os_and_arch.os == "macos" {
            fs::rename(
                cef_dir.join("cef_sandbox.a"),
                cef_dir.join("libcef_sandbox.a"),
            )?;
        } else {
            let resources = extracted_dir.join("Resources");

            for entry in fs::read_dir(&resources)? {
                let entry = entry?;
                fs::rename(entry.path(), cef_dir.join(entry.file_name()))?;
            }
        }

        const CMAKE_LISTS_TXT: &str = "CMakeLists.txt";
        fs::rename(
            extracted_dir.join(CMAKE_LISTS_TXT),
            cef_dir.join(CMAKE_LISTS_TXT),
        )?;
        const CMAKE_DIR: &str = "cmake";
        fs::rename(extracted_dir.join(CMAKE_DIR), cef_dir.join(CMAKE_DIR))?;
        const INCLUDE_DIR: &str = "include";
        fs::rename(extracted_dir.join(INCLUDE_DIR), cef_dir.join(INCLUDE_DIR))?;
        const LIBCEF_DLL_DIR: &str = "libcef_dll";
        fs::rename(
            extracted_dir.join(LIBCEF_DLL_DIR),
            cef_dir.join(LIBCEF_DLL_DIR),
        )?;

        if show_progress {
            println!("Moved contents to: {}", cef_dir.display());
        }

        // Cleanup whatever is left in the extracted directory.
        let old_dir = extracted_dir
            .parent()
            .map(|parent| {
                parent.join(format!(
                    "extracted_{}_{}",
                    self.os_and_arch.os, self.os_and_arch.arch
                ))
            })
            .ok_or_else(|| Error::InvalidArchiveFile(extracted_dir.display().to_string()))?;
        if show_progress {
            println!("Cleaning up: {}", old_dir.display());
        }
        fs::rename(&extracted_dir, &old_dir)?;
        fs::remove_dir_all(old_dir)?;

        Ok(cef_dir)
    }
}

impl TTCefVersion {
    pub fn from(target: &str, tt_version: &str) -> Self {
        let base = "https://voffline.byted.org/download/tos/schedule//tt-cef/distribution";
        let os_and_arch = OsAndArch::try_from(target)
            .map_err(|err| Error::UnsupportedTarget(err.to_string()))
            .unwrap();
        let OsAndArch { os, arch } = os_and_arch;

        // 固定版本号
        // let version = "135.7.1.release.main+rs-135.68";
        match (os, arch) {
            ("macos", "x86_64") => TTCefVersion {
                url: format!("{base}/{tt_version}/mac_x64/Release_GN_x64.tar.bz2"),
                file: CefFile {
                    file_type: "minimal".into(),
                    name: format!("cef_binary_{tt_version}_mac_x64_Release_GN_x64.tar.bz2"),
                    sha1: "".into(),
                },
                os_and_arch,
                target: target.into(),
            },
            ("macos", "aarch64") => TTCefVersion {
                url: format!("{base}/{tt_version}/mac_arm64/Release_GN_arm64.tar.bz2"),
                file: CefFile {
                    file_type: "minimal".into(),
                    name: format!("cef_binary_{tt_version}_mac_arm64_Release_GN_arm64.tar.bz2"),
                    sha1: "".into(),
                },
                os_and_arch,
                target: target.into(),
            },
            ("windows", "i686") => TTCefVersion {
                url: format!("{base}/{tt_version}/win_x64/Release_GN_x86.tar.bz2"),
                file: CefFile {
                    file_type: "minimal".into(),
                    name: format!("cef_binary_{tt_version}_win_x64_Release_GN_x86.tar.bz2"),
                    sha1: "".into(),
                },
                os_and_arch,
                target: target.into(),
            },
            ("windows", "x86_64") => TTCefVersion {
                url: format!("{base}/{tt_version}/win_x64/Release_GN_x64.tar.bz2"),
                file: CefFile {
                    file_type: "minimal".into(),
                    name: format!("cef_binary_{tt_version}win_x64_Release_GN_x64.tar.bz2"),
                    sha1: "".into(),
                },
                os_and_arch,
                target: target.into(),
            },
            _ => panic!("not found url"),
        }
    }
}

pub fn map_arch_to_ttcef(arch: &str) -> &'static str {
    match arch {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        "i686" => "x86",
        "arm" => "arm",
        _ => panic!("unsupport"),
    }
}

#[derive(Deserialize, Serialize)]
pub struct CefFile {
    #[serde(rename = "type")]
    pub file_type: String,
    pub name: String,
    pub sha1: String,
}
