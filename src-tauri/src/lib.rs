pub mod webvtt;
use dotenv::dotenv;
use tauri::{Emitter, Manager};
mod db;
mod gemini;
mod setting;
mod utils;
mod whisper;
use tube_rs::YoutubeAudio;

#[tauri::command(rename_all = "snake_case")]
async fn run_yt(app: tauri::AppHandle, url: &str, input_id: i64) -> Result<(), String> {
    let mut _id = input_id;
    let youtube_audio = YoutubeAudio::new(
        setting::get_proxy(&app).as_deref(),
        setting::get_tube_api_url(&app),
    );
    if _id == -1 {
        let audio_data = match youtube_audio.get_video_info(url).await {
            Some(data) => data,
            None => return Err("failed to parse audio info".to_string()),
        };
        _id = db::create_video(app.state(), audio_data)?;
        app.emit("state", "update video")
            .map_err(|e| e.to_string())?;
    };
    if let Ok((Some(lang), Some(url))) = db::get_caption_with_id(app.state(), _id) {
        let subtitles = youtube_audio
            .download_caption(&url, &lang)
            .await
            .map_err(|e| e.to_string())?;
        app.emit("stream", "[start]".to_string())
            .map_err(|e| e.to_string())?;
        for subtitle in &subtitles {
            app.emit("stream", subtitle.text.clone())
                .map_err(|e| e.to_string())?
        }
        app.emit("stream", "[end]".to_string())
            .map_err(|e| e.to_string())?;

        let segments = utils::transform_subtitles_to_segments(subtitles);
        let transcripts = serde_json::to_string(&segments).unwrap();
        db::update_video(app.state(), _id, "transcripts".to_string(), transcripts)?;
        return Ok(());
    };

    let (audio_url, audio_filesize, mime_type, duration) =
        db::get_audio_url_with_id(app.state(), _id)?;
    let cache_dir = app.path().cache_dir().unwrap();
    let file_path = if mime_type.contains("webm") {
        "temp.webm"
    } else {
        "temp.m4a"
    };
    let mut temp_path = cache_dir.join("newscenter").join(file_path);
    youtube_audio
        .download_audio(&audio_url, audio_filesize, &temp_path)
        .await
        .map_err(|e| e.to_string())?;

    if audio_filesize > 22 * 1024 * 1024 {
        let output_dir = cache_dir.join("chunk");
        let bytes_per_second = audio_filesize as f64 / duration as f64;
        let chunk_duration = ((20 * 1024 * 1024) as f64 / bytes_per_second) as i64;

        let auido_splitter = ffmpeg_audio::AudioSplitter::new(chunk_duration);
        auido_splitter
            .split(&temp_path, &output_dir)
            .map_err(|e| e.to_string())?;

        temp_path = output_dir;
    };

    app.emit("stream", "[start]".to_string())
        .map_err(|e| e.to_string())?;
    let segments = whisper::trancript(&app, &temp_path).await?;
    app.emit("stream", "[end]".to_string())
        .map_err(|e| e.to_string())?;
    let transcripts = serde_json::to_string(&segments).unwrap();
    db::update_video(app.state(), _id, "transcripts".to_string(), transcripts)?;

    Ok(())
}

#[tauri::command]
async fn fetch_image(app: tauri::AppHandle, url: String) -> Result<Vec<u8>, String> {
    let client = whisper::create_client(&app)
        .await
        .map_err(|e| e.to_string())?;

    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    Ok(bytes.to_vec())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    dotenv().ok();
    tauri::Builder::default()
        .setup(|app| {
            setting::get_config_path(app.handle());
            let database = db::init_db(app.handle())?;
            app.manage(database);
            Ok(())
        })
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            run_yt,
            fetch_image,
            whisper::run_summary,
            db::get_videos,
            db::delete_video,
            db::clear_all,
            setting::load_settings,
            setting::save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
