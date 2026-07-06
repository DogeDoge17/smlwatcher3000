use tokio::time::{sleep, Duration};
use thirtyfour::{FirefoxCapabilities, prelude::*};
#[cfg(target_os = "windows")]
use std::fmt::format;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::path;
use std::path::PathBuf;
use std::ffi::OsStr;
use std::env;
use std::collections::HashSet;
use std::process::exit;
use std::sync::{LazyLock, Mutex};
use rand::prelude::*;

static POTENTIAL_VIDEOS: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

const APP_NAME: &str = "smlwatcher3000";

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn data_dir() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".local/share")))
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join(APP_NAME)
}

fn video_ids_path() -> PathBuf {
    let local_path = PathBuf::from("video-ids.txt");
    if local_path.exists() {
        local_path
    } else {
        // shoddy for now but later ill have it not depend on an external script to fetch the video ids and just do it in rust
        // maybe presets if i feel like it or sum idk
        data_dir().join("video-ids.txt")
    }
}

fn watched_path() -> PathBuf {
    data_dir().join("watched.txt")
}

async fn prep_new_video(driver: &WebDriver, video_id: &str) -> anyhow::Result<()> {
    driver
        .goto(format!("https://www.youtube.com/watch?v={}", video_id))
        .await?;
    driver.refresh().await?;

    let video = driver.find(By::Css("#player")).await?;
    video.click().await?;

    let ten_millis = Duration::from_millis(500);
    sleep(ten_millis).await;

    driver
        .action_chain()
        .double_click_element(&video)
        .perform()
        .await?;

    Ok(())
}

async fn check_video_length(driver: &WebDriver) -> anyhow::Result<()> {
    let video_element = driver.find(By::Tag("video")).await?;
    let url_whatever = driver.current_url().await?;
    let url = url_whatever.as_str();

    loop {
        let ready_state: i64 = driver
            .execute(
                "return arguments[0].readyState;",
                vec![video_element.to_json()?],
            )
            .await?
            .convert()?;

        if ready_state >= 1 {
            break;
        }

        sleep(Duration::from_millis(100)).await;
    }

    loop {
        let duration: f64 = match driver
            .execute(
                "return arguments[0].duration;",
                vec![video_element.to_json()?],
            )
            .await
            .and_then(|value| value.convert())
        {
            Ok(duration) => duration,
            Err(_) => break,
        };

        let current_time: f64 = match driver
            .execute(
                "return arguments[0].currentTime;",
                vec![video_element.to_json()?],
            )
            .await
            .and_then(|value| value.convert())
        {
            Ok(current_time) => current_time,
            Err(_) => break,
        };

        if current_time >= duration || driver.current_url().await?.as_str() != url {
            break;
        }

        // i wish it could check less often but youtube auto plays too fast
        // youre already watching a video though (very expensive) so its not that deep
        sleep(Duration::from_millis(2800)).await;
    }

    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.file_name() == Some(OsStr::new("lock")) {
            continue;
        }

        if file_type.is_dir() {
            match copy_dir(&src_path, &dst_path) {
                Ok(it) => it,
                Err(_) => println!("{:?}", src_path),
            };
        } else {
            let _ = match fs::copy(&src_path, &dst_path) {
                Ok(it) => Ok(it),
                Err(err) => {
                    println!("{:?}", src_path);
                    Err(err)
                }
            };
        }
    }
    Ok(())
}

async fn prep_gecko_profile(caps: &mut FirefoxCapabilities) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    let profile_paths = {
        let home = home_dir().ok_or_else(|| anyhow::anyhow!("HOME or USERPROFILE is not set"))?;
        [
            home.join("AppData/Roaming/Mozilla/Firefox/Profiles"),
            home.join("AppData/Local/Packages/Mozilla.Firefox_4v1f9aba9rhr4/LocalCache/Roaming/Mozilla/Firefox/Profiles"),
        ]
    };
    #[cfg(target_os = "linux")]
    let profile_paths = {
        let home = home_dir().ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
        [
            home.join(".mozilla/firefox"),
            home.join(".config/mozilla/firefox"),
            home.join("snap/firefox/common/.mozilla/firefox"),
            home.join(".var/app/org.mozilla.firefox/.mozilla/firefox"),
        ]
    };
    #[cfg(target_os = "macos")]
    let profile_paths = {
        let home = home_dir().ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
        [
            home.join("Library/Application Support/Firefox/Profiles"),
            home.join(".config/.mozilla/firefox"),
        ]
    };

    let profile_path = env::temp_dir().join("smlwatcher3000-gecko");
    fs::create_dir_all(&profile_path)?;

    // find the folder w the profile
    for potential_root_folder in profile_paths {
        let path = path::Path::new(&potential_root_folder);
        if !path.exists() {
            continue;
        }

        let found_potential_profile_folders: Vec<path::PathBuf> = match fs::read_dir(path) {
            Ok(entries) => entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| match entry.file_type() {
                    Ok(file_type) if file_type.is_dir() => Some(entry.path()),
                    _ => None,
                })
                .collect(),
            Err(_) => continue,
        };
        
        let found_profile = found_potential_profile_folders.into_iter().find(|potential_folders| {
            fs::read_dir(potential_folders)
                .map(|entries| {
                    entries
                        .filter_map(|entry| entry.ok())
                        .any(|entry| entry.file_name() == "extensions.json")
                })
                .unwrap_or(false)
        });
        
        let Some(found_profile) = found_profile else {
            continue;
        };

        // TODO: move to after when ad block extension is found.
        fs::create_dir_all(&profile_path)?;
        copy_dir(&found_profile, &profile_path)?;
        caps.add_arg("-profile")?;
        caps.add_arg(
            profile_path
                .to_str()
                .ok_or(anyhow::anyhow!("Firefox profile path is not valid UTF-8"))?,
        )?;
    }
    
    Ok(())
}

fn blacklist_video(video_id: &String) -> anyhow::Result<()> {
    if let Some(parent) = watched_path().parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(watched_path())?;
    writeln!(file,"{}", video_id)?;   

    Ok(())
}

fn next_video() -> anyhow::Result<String> {
    let mut rng = rand::rng();
    let index = rng.random_range(0..POTENTIAL_VIDEOS.lock().unwrap().len());
    let removed = POTENTIAL_VIDEOS.lock().unwrap().swap_remove(index);

    Ok(removed)
}

fn prep_whitelist() -> anyhow::Result<()> {
    let video_ids_path = video_ids_path();
    let video_ids_contents = match fs::read_to_string(&video_ids_path) {
        Ok(it) => it,
       Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("holy derp you dont have any videos to watch. Please create a new-line separated list of video ids at {:?} or ./video-ids.txt\nSee fetchinfo.py for a script to auto scrape a channel of all its videos", video_ids_path);
            exit(0);
        },
        Err(e) => return Err(e.into()),
    };
  
    let watched_path = watched_path();
    let watched_videos_contents = match fs::read_to_string(&watched_path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = watched_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::File::create(&watched_path)?;
            String::new()
        }
        Err(e) => return Err(e.into()),
    };

    let blacklist: HashSet<String> = watched_videos_contents
          .lines()
          .map(|s| s.to_string())
          .collect();


    *POTENTIAL_VIDEOS.lock().unwrap() = video_ids_contents
        .lines()
        .filter(|s| !blacklist.contains(*s))
        .map(|s| s.to_string())
        .collect();
    // println!("{}", POTENTIAL_VIDEOS.lock().unwrap().len());

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // let mut pargs = pico_args::Arguments::from_env();
    let mut caps = DesiredCapabilities::firefox();

    let gecko_prep = prep_gecko_profile(&mut caps);

    prep_whitelist()?;

    gecko_prep.await?;

    let driver = WebDriver::managed(caps).await?;

    loop {
        let video_id: String = next_video()?;
        prep_new_video(&driver, &video_id).await?;
        check_video_length(&driver).await?;

        // blacklist the video after because if you quit early, 99% chance you didnt finish watching
        blacklist_video(&video_id)?;
    };

    // no cleanup because why would you want to ever stop watching
}
