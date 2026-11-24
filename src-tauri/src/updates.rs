use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::Duration;
#[cfg(desktop)]
use std::time::Instant;
use tauri::{AppHandle, State, ipc::Channel};
#[cfg(desktop)]
use tauri_plugin_updater::{Update, UpdaterExt};

const REPOSITORY: &str = "https://github.com/zzxCNCZ/oh-my-chatwise";
const LATEST: &str = "https://api.github.com/repos/zzxCNCZ/oh-my-chatwise/releases/latest";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

#[derive(Serialize)]
pub struct UpdateStatus {
    version: Option<String>,
    installable: bool,
}

#[derive(Default)]
pub struct InstallLock(tokio::sync::Mutex<()>);

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InstallProgress {
    Downloading { received: u64, total: Option<u64> },
    Installing,
}

fn is_new_stable(current: &Version, latest: &Version) -> bool {
    latest.pre.is_empty() && latest.cmp_precedence(current).is_gt()
}

fn newer_release(current: &Version, release: Release) -> Result<Option<String>, String> {
    let tag = release.tag_name;
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(&tag))
        .map_err(|_| "GitHub 返回了无效的发布版本。")?;
    Ok(is_new_stable(current, &version).then(|| version.to_string()))
}

fn is_release_asset(version: &str, url: &str) -> bool {
    url.strip_prefix(REPOSITORY)
        .and_then(|path| path.strip_prefix("/releases/download/"))
        .and_then(|path| path.split_once('/'))
        .is_some_and(|(tag, filename)| {
            tag.strip_prefix('v').unwrap_or(tag) == version
                && !filename.is_empty()
                && !filename.contains(['/', '?', '#'])
        })
}

#[cfg(target_os = "macos")]
fn in_application_bundle(executable: &std::path::Path) -> bool {
    executable.parent().is_some_and(|directory| {
        directory.ends_with("Contents/MacOS")
            && directory
                .parent()
                .and_then(std::path::Path::parent)
                .and_then(std::path::Path::extension)
                .is_some_and(|extension| extension == "app")
    })
}

#[cfg(desktop)]
async fn signed_package(app: &AppHandle) -> Result<Option<Update>, String> {
    if !app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|config| config["pubkey"].as_str())
        .is_some_and(|key| !key.is_empty())
    {
        return Err("此版本未配置应用内更新签名。".into());
    }
    let builder = app
        .updater_builder()
        .timeout(Duration::from_secs(10))
        .version_comparator(|current, release| is_new_stable(&current, &release.version));
    #[cfg(target_os = "macos")]
    let builder = {
        // Tauri can otherwise replace the executable's parent outside an app bundle.
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        if !in_application_bundle(&executable) {
            return Err("请在已安装的应用中下载并安装更新。".into());
        }
        builder.executable_path(executable)
    };
    let updater = builder.build().map_err(|error| error.to_string())?;
    let package = updater.check().await.map_err(|error| error.to_string())?;
    if let Some(package) = &package
        && !is_release_asset(&package.version, package.download_url.as_str())
    {
        return Err("更新包地址与项目发布版本不一致。".into());
    }
    Ok(package)
}

#[tauri::command]
pub async fn check_updates(app: AppHandle) -> Result<UpdateStatus, String> {
    #[cfg(desktop)]
    if let Ok(package) = signed_package(&app).await {
        return Ok(UpdateStatus {
            installable: package.is_some(),
            version: package.map(|package| package.version),
        });
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("oh-my-chatwise/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| error.to_string())?;
    let release = client
        .get(LATEST)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| error.to_string())?
        .json::<Release>()
        .await
        .map_err(|error| error.to_string())?;
    Ok(UpdateStatus {
        version: newer_release(&app.package_info().version, release)?,
        installable: false,
    })
}

#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    lock: State<'_, InstallLock>,
    version: String,
    progress: Channel<InstallProgress>,
) -> Result<(), String> {
    #[cfg(not(desktop))]
    {
        let _ = (app, lock, version, progress);
        return Err("当前平台不支持应用内安装。".into());
    }
    #[cfg(desktop)]
    {
        let _guard = lock.0.try_lock().map_err(|_| "已有更新正在安装。")?;
        let mut package = signed_package(&app)
            .await?
            .filter(|package| package.version == version)
            .ok_or("可用版本已变化，请重新检查更新。")?;
        package.timeout = Some(Duration::from_secs(600));
        let mut received = 0;
        let mut last_report = None;
        package
            .download_and_install(
                |length, total| {
                    received += length as u64;
                    let now = Instant::now();
                    if last_report.is_none_or(|last| now - last >= Duration::from_millis(100))
                        || total == Some(received)
                    {
                        last_report = Some(now);
                        let _ = progress.send(InstallProgress::Downloading { received, total });
                    }
                },
                || {
                    let _ = progress.send(InstallProgress::Installing);
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        // On Windows the installer relaunches the app.
        #[cfg(not(target_os = "windows"))]
        app.request_restart();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_only_newer_stable_versions() {
        let current = Version::parse("1.9.0").unwrap();
        let result = |tag: &str| {
            newer_release(
                &current,
                Release {
                    tag_name: tag.into(),
                },
            )
        };
        assert_eq!(result("v1.10.0").unwrap().as_deref(), Some("1.10.0"));
        for tag in ["1.9.0", "1.9.0+other", "1.8.0", "2.0.0-rc.1"] {
            assert!(result(tag).unwrap().is_none());
        }
        assert!(result("../other").is_err());
    }

    #[test]
    fn accepts_only_assets_from_the_matching_release() {
        let path = |tag| format!("{REPOSITORY}/releases/download/{tag}/package.app.tar.gz");
        assert!(is_release_asset("2.1.0", &path("v2.1.0")));
        assert!(!is_release_asset("2.1.0", &path("v2.0.0")));
        assert!(!is_release_asset(
            "2.1.0",
            &path("v2.1.0").replace("zzxCNCZ", "other")
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn only_installs_into_an_application_bundle() {
        use std::path::Path;
        assert!(in_application_bundle(Path::new(
            "/Applications/oh-my-chatwise.app/Contents/MacOS/oh-my-chatwise"
        )));
        assert!(!in_application_bundle(Path::new(
            "/project/Contents/MacOS/oh-my-chatwise"
        )));
        assert!(!in_application_bundle(Path::new(
            "/project/target/debug/oh-my-chatwise"
        )));
    }
}
