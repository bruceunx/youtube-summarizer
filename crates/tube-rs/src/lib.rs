use regex::Regex;
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT_LANGUAGE, CONTENT_TYPE, USER_AGENT},
    Client, Proxy,
};
use serde::{Deserialize, Serialize};
use serde_xml_rs::from_str;
use std::{error::Error, fs::File, io::Write, path::Path, time::Duration};

pub struct YoutubeAudio {
    client: Client,
    tube_api_url: Option<String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ClientInfo {
    client_name: String,
    os_name: String,
    os_version: String,
    client_version: String,
    platform: String,
    visitor_data: String,
}

impl ClientInfo {
    pub fn default(visitor_data: String) -> Self {
        Self {
            client_name: "WEB".to_string(),
            os_name: "Windows".to_string(),
            os_version: "10.0".to_string(),
            client_version: "2.20240726.00.00".to_string(),
            platform: "DESKTOP".to_string(),
            visitor_data,
        }
    }
}

#[derive(Debug)]
struct AuthData {
    visitor_data: String,
    signature_timestamp: String,
    po_token: Option<String>,
}

// request data

#[derive(Serialize, Debug)]
struct RequestContext {
    client: ClientInfo,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct SignatureTimestamp {
    signature_timestamp: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct PlaybackContext {
    content_playback_context: SignatureTimestamp,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ServiceIntegrityDimensions {
    po_token: Option<String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct RequestBody {
    context: RequestContext,
    video_id: String,
    content_check_ok: String,
    playback_context: PlaybackContext,
    service_integrity_dimensions: ServiceIntegrityDimensions,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseBody {
    streaming_data: StreamingData,
    captions: Option<Captions>,
    video_details: VideoDetail,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoDetail {
    // video_id: String,
    title: String,
    length_seconds: String,
    keywords: Option<Vec<String>>,
    short_description: Option<String>,
    // thumbnail: ThumbNail,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Captions {
    player_captions_tracklist_renderer: PlayerCaptionsTracklistRenderer,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerCaptionsTracklistRenderer {
    caption_tracks: Vec<CaptionItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptionItem {
    base_url: String,
    vss_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Format {
    mime_type: String,
    bitrate: u32,
    url: String,
    content_length: String,
    last_modified: String,
}

#[derive(Deserialize)]
struct StreamingData {
    // formats: Option<Vec<Format>>,
    #[serde(rename = "adaptiveFormats")]
    adaptive_formats: Option<Vec<Format>>,
}

#[derive(Deserialize, Debug)]
struct AudioStream {
    url: String,
    filesize: u64,
    mime_type: String,
}

#[derive(Deserialize, Debug)]
struct AudioResponsBody {
    audio_stream: AudioStream,
}

// export the struct
#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AudioData {
    pub video_id: String,
    pub title: String,
    pub duration: u64,
    pub timestamp: u64,
    pub keywords: Option<Vec<String>>,
    pub description: Option<String>,
    pub caption_lang: Option<String>,
    pub caption_url: Option<String>,
    pub audio_url: String,
    pub audio_filesize: u64,
    pub thumbnail_url: String,
    pub mime_type: String,
}

pub struct SubtitleEntry {
    pub timestamp: u64,
    pub duration: u32,
    pub text: String,
}

fn parse_auth_from_content(pattern: &str, content: &str) -> Result<String, String> {
    if let Some(capture) = Regex::new(pattern).unwrap().captures(content) {
        let value = match capture.get(1).map(|m| m.as_str().to_string()) {
            Some(data) => Ok(data),
            None => return Err("failed to parse js_url".to_string()),
        };
        value
    } else {
        Err("faild to parse the pattern from html".to_string())
    }
}

fn extract_id(url: &str) -> Option<String> {
    let re = Regex::new(r"(?:v=|\/v\/|youtu\.be\/|\/embed\/|\/shorts\/)([A-Za-z0-9_-]+)").unwrap();

    if let Some(captures) = re.captures(url) {
        return captures.get(1).map(|m| m.as_str().to_string());
    }
    None
}

fn preprocess_xml(xml_content: &str) -> String {
    // Remove XML declaration
    let without_declaration = xml_content
        .trim()
        .replace(r#"<?xml version="1.0" encoding="utf-8" ?>"#, "")
        .replace(r#"<?xml version='1.0' encoding='UTF-8'?>"#, "")
        .trim()
        .to_string();

    without_declaration
}

async fn get_auth_audio_link(video_id: &str, api_url: &str) -> Option<AudioStream> {
    #[derive(Serialize)]
    struct AuidoRequestBody {
        url: String,
    }

    let client = Client::new();
    let body = AuidoRequestBody {
        url: format!("https://www.youtube.com/watch?v={video_id}"),
    };

    match client.post(api_url).json(&body).send().await {
        Ok(res) => match res.json::<AudioResponsBody>().await {
            Ok(data) => Some(data.audio_stream),
            Err(_) => None,
        },
        Err(_) => None,
    }
}

fn parse_xml(xml: &str) -> Vec<SubtitleEntry> {
    // Parse the XML content
    #[derive(Debug, Deserialize)]
    struct Transcript {
        #[serde(rename = "text")]
        entries: Vec<TextEntry>,
    }

    #[derive(Debug, Deserialize)]
    struct TextEntry {
        #[serde(rename = "start")]
        start: f64,
        #[serde(rename = "dur")]
        duration: f64,
        #[serde(rename = "$value")]
        text: String,
    }

    let cleaned_xml = preprocess_xml(xml);
    // Deserialize the XML
    let transcript: Transcript = match from_str(&cleaned_xml) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error parsing XML: {:?}", e);
            return Vec::new();
        }
    };

    // Convert to SubtitleEntry
    transcript
        .entries
        .into_iter()
        .map(|entry| SubtitleEntry {
            timestamp: (entry.start * 1000.0) as u64, // convert to milliseconds
            duration: (entry.duration * 1000.0) as u32, // convert to milliseconds
            text: entry.text,
        })
        .collect()
}

impl YoutubeAudio {
    pub fn new(proxy: Option<&str>, tube_api_url: Option<String>) -> Self {
        let client_builder = Client::builder();
        let client = match proxy {
            Some(proxy_str) => match Proxy::https(proxy_str) {
                Ok(ok_proxy) => client_builder.proxy(ok_proxy).build().unwrap(),
                Err(_) => client_builder.build().unwrap(),
            },
            _ => client_builder.build().unwrap(),
        };
        Self {
            client,
            tube_api_url,
        }
    }

    async fn get_auth_info(&self, video_id: &str) -> Result<AuthData, String> {
        let url = format!("https://www.youtube.com/watch?v={video_id}");
        // parse js_url
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let html = response.text().await.map_err(|e| e.to_string())?;
        let js_pattern = r"(/s/player/[\w\d]+/[\w\d_/.]+/base\.js)";
        let signature_pattern = r"signatureTimestamp:(\d*)";
        let visitor_data_pattern = r#"\{"key":"visitor_data","value":"([^"]+)"\}"#;

        let js_partial_url = parse_auth_from_content(js_pattern, &html)?;
        let js_url = format!("https://www.youtube.com{js_partial_url}");

        let response_js = self
            .client
            .get(&js_url)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let js_html = response_js.text().await.map_err(|e| e.to_string())?;

        let signature_timestamp = parse_auth_from_content(signature_pattern, &js_html)?;

        let visitor_data = parse_auth_from_content(visitor_data_pattern, &html)?;

        // let po_token = generate_po_token(&visitor_data).await;

        Ok(AuthData {
            po_token: None,
            signature_timestamp,
            visitor_data,
        })
    }

    pub async fn get_video_info(&self, url: &str) -> Option<AudioData> {
        let video_id = match extract_id(url) {
            Some(_id) => _id,
            None => return None,
        };

        let auth_data = self.get_auth_info(&video_id).await.ok()?;

        let mut headers = HeaderMap::new();

        headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0"));
        headers.insert("X-Youtube-Client-Name", HeaderValue::from_static("1"));
        headers.insert(
            "X-Youtube-Client-Version",
            HeaderValue::from_static("2.20240726.00.00"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let request_body = RequestBody {
            context: RequestContext {
                client: ClientInfo::default(auth_data.visitor_data),
            },
            playback_context: PlaybackContext {
                content_playback_context: SignatureTimestamp {
                    signature_timestamp: auth_data.signature_timestamp,
                },
            },
            service_integrity_dimensions: ServiceIntegrityDimensions {
                po_token: auth_data.po_token,
            },
            video_id: video_id.to_string(),
            content_check_ok: "true".to_string(),
        };

        const MAX_RETRIES: u32 = 3;
        const INITIAL_BACKOFF_MS: u64 = 1000;

        let mut attempt = 0;
        let response_data: ResponseBody = loop {
            attempt += 1;

            match self
                .client
                .post("https://www.youtube.com/youtubei/v1/player?prettyPrint=false")
                .headers(headers.clone())
                .json(&request_body)
                .send()
                .await
            {
                Ok(response) => match response.json().await {
                    Ok(data) => break data,
                    Err(e) => {
                        eprintln!("Faled to parse the info from response {e}");

                        if attempt >= MAX_RETRIES {
                            return None;
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Failed to get response from youtube {e}");
                    if attempt >= MAX_RETRIES {
                        return None;
                    }
                }
            }

            let backoff_duration =
                Duration::from_millis(INITIAL_BACKOFF_MS * 2u64.pow(attempt - 1));
            tokio::time::sleep(backoff_duration).await;
        };

        let mut all_formats = Vec::new();

        if let Some(adaptive_formats) = response_data.streaming_data.adaptive_formats {
            all_formats.extend(adaptive_formats);
        }

        let (mut mime_type, last_modified, mut audio_url, mut audio_filesize) = match all_formats
            .into_iter()
            .filter(|format| format.mime_type.starts_with("audio"))
            .min_by_key(|format| format.bitrate)
        {
            Some(format) => (
                format.mime_type,
                format.last_modified.parse::<u64>().unwrap(),
                format.url,
                format.content_length.parse::<u64>().ok().unwrap_or(0),
            ),

            _ => return None,
        };

        let (caption_url, caption_lang) = match response_data.captions {
            Some(captions) => {
                let caption_array = captions.player_captions_tracklist_renderer.caption_tracks;

                let caption = if caption_array.len() > 1 {
                    let caption_en = caption_array.iter().find(|item| item.vss_id.contains("en"));
                    match caption_en {
                        Some(caption) => caption,
                        None => &caption_array[0],
                    }
                } else {
                    &caption_array[0]
                };

                (Some(caption.base_url.clone()), Some(caption.vss_id.clone()))
            }
            _ => (None, None),
        };

        if caption_lang.is_none() && self.tube_api_url.is_some() {
            let api_url = self.tube_api_url.clone().unwrap();
            if let Some(audio_data) = get_auth_audio_link(&video_id, &api_url).await {
                audio_url = audio_data.url;
                audio_filesize = audio_data.filesize;
                mime_type = audio_data.mime_type;
            }
        }

        let thumbnail_url = format!("https://i.ytimg.com/vi/{}/sddefault.jpg", video_id);

        Some(AudioData {
            video_id,
            title: response_data.video_details.title,
            duration: response_data
                .video_details
                .length_seconds
                .parse::<u64>()
                .unwrap(),
            timestamp: last_modified,
            keywords: response_data.video_details.keywords,
            description: response_data.video_details.short_description,
            caption_lang,
            caption_url,
            audio_url,
            audio_filesize,
            mime_type,
            thumbnail_url,
        })
    }

    pub async fn download_caption(
        &self,
        caption_url: &str,
        _caption_lang: &str,
    ) -> Result<Vec<SubtitleEntry>, Box<dyn Error>> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0"));
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-us,en"));
        let response = self.client.get(caption_url).headers(headers).send().await?;
        let xml = response.text().await?;
        Ok(parse_xml(&xml))
    }

    pub async fn download_audio(
        &self,
        audio_url: &str,
        file_size: u64,
        file_path: &Path,
    ) -> Result<(), Box<dyn Error>> {
        const MAX_RETRIES: u32 = 3;
        const INITIAL_BACKOFF_MS: u64 = 1000;
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0"));
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-us,en"));
        let mut file = File::create(file_path)?;
        let mut downloaded = 0;
        let default_range_size = 1024 * 1024 * 9;
        let mut attempt = 0;
        while downloaded < file_size {
            let stop_pos = (downloaded + default_range_size).min(file_size) - 1;

            let chunk = loop {
                attempt += 1;
                match self
                    .client
                    .get(format!("{}?range={}-{}", audio_url, downloaded, stop_pos))
                    .headers(headers.clone())
                    .send()
                    .await
                {
                    Ok(chunk_reponse) => match chunk_reponse.bytes().await {
                        Ok(data) => break data,
                        Err(e) => {
                            eprintln!("Failed to parse the bytes from response {e}");
                            if attempt >= MAX_RETRIES {
                                return Err(Box::new(e));
                            }
                        }
                    },
                    Err(e) => {
                        eprintln!("Failed to get the response {e}");
                        if attempt >= MAX_RETRIES {
                            return Err(Box::new(e));
                        }
                    }
                }
                let backoff_duration =
                    Duration::from_millis(INITIAL_BACKOFF_MS * 2u64.pow(attempt - 1));
                tokio::time::sleep(backoff_duration).await;
            };

            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dotenv::dotenv;
    use std::path::PathBuf;
    use std::{env, str::FromStr};

    #[tokio::test]
    async fn check_get_audio_link() {
        let tube_api_url = env::var("TUBE_API_URL").unwrap();
        let result = get_auth_audio_link("jcrE1qrm_e8", &tube_api_url).await;
        assert!(result.is_some());
        println!("{result:?}");
    }

    #[tokio::test]
    async fn check_caption_lang_works() {
        dotenv().ok();
        let proxy = env::var("PROXY").ok();
        let tube_api_url = env::var("TUBE_API_URL").ok();
        let youtube_client = YoutubeAudio::new(proxy.as_deref(), tube_api_url);
        let url = "https://www.youtube.com/watch?v=2p_Hlm6aCok&ab_channel=TheoriesofEverythingwithCurtJaimungal";
        let video_data = youtube_client.get_video_info(url).await;
        assert!(video_data.is_some());
        let video = video_data.unwrap();
        assert!(video.caption_lang.unwrap().contains("en"));
    }

    #[tokio::test]
    async fn check_response_body_works() {
        dotenv().ok();
        let proxy = env::var("PROXY").ok();
        let tube_api_url = env::var("TUBE_API_URL").ok();
        let youtube_client = YoutubeAudio::new(proxy.as_deref(), tube_api_url);
        let url = "https://www.youtube.com/watch?v=s78hvV3QLUE&ab_channel=LexFridman";
        let video_data = youtube_client.get_video_info(url).await;
        assert!(video_data.is_some());
        // let video = video_data.unwrap();
        // assert_eq!(video.caption_lang.unwrap(), "a.en".to_string());
        // assert!(video.timestamp > 0);
    }

    #[tokio::test]
    async fn check_download_audio_works() {
        dotenv().ok();
        let proxy = env::var("PROXY").ok();
        let tube_api_url = env::var("TUBE_API_URL").ok();
        let youtube_client = YoutubeAudio::new(proxy.as_deref(), tube_api_url);
        let url = "https://www.youtube.com/watch?v=s78hvV3QLUE&t=4s"; //"https://www.youtube.com/watch?v=Q0cvzaPJJas&ab_channel=TJDeVries";
        let video_data = youtube_client.get_video_info(url).await;
        assert!(video_data.is_some());

        let video = video_data.unwrap();

        let audio_url = &video.audio_url;
        let audio_length = video.audio_filesize;
        let file_path = if video.mime_type.contains("webm") {
            PathBuf::from_str("./sample.webm").unwrap()
        } else {
            PathBuf::from_str("./sample.m4a").unwrap()
        };
        let download = youtube_client
            .download_audio(audio_url, audio_length, &file_path)
            .await;

        assert!(download.is_ok());
    }

    #[test]
    fn extract_id_works() {
        let test_cases = vec![
            (
                "https://youtu.be/FdeioVndUhs",
                Some("FdeioVndUhs".to_string()),
            ), // Short YouTube URL
            (
                "https://www.youtube.com/watch?v=FdeioVndUhs",
                Some("FdeioVndUhs".to_string()),
            ), // Standard YouTube URL
            (
                "https://youtube.com/watch?v=FdeioVndUhs",
                Some("FdeioVndUhs".to_string()),
            ), // YouTube without 'www'
            (
                "https://www.youtube.com/v/FdeioVndUhs",
                Some("FdeioVndUhs".to_string()),
            ), // /v/ style URL
            (
                "https://www.youtube.com/watch?v=s78hvV3QLUE&ab_channel=LexFridman",
                Some("s78hvV3QLUE".to_string()),
            ), // /v/ style URL
            (
                "https://www.youtube.com/embed/FdeioVndUhs",
                Some("FdeioVndUhs".to_string()),
            ), // Embedded URL
            (
                "https://www.youtube.com/shorts/FdeioVndUhs",
                Some("FdeioVndUhs".to_string()),
            ), // Shorts URL
            (
                "https://youtu.be/FdeioVndUhs?t=30",
                Some("FdeioVndUhs".to_string()),
            ), // URL with timestamp
            (
                "https://www.youtube.com/watch?v=FdeioVndUhs&feature=share",
                Some("FdeioVndUhs".to_string()),
            ), // With additional params
            (
                "https://www.youtube.com/watch?v=FdeioVndUhs&list=PL123",
                Some("FdeioVndUhs".to_string()),
            ), // With playlist
            (
                "https://www.youtube.com/watch?list=PL123&v=FdeioVndUhs",
                Some("FdeioVndUhs".to_string()),
            ), // Playlist first
            ("https://invalid.url/FdeioVndUhs", None), // Invalid URL
            ("https://www.youtube.com/", None),        // Homepage
            ("https://youtu.be/", None),               // Invalid Short URL
        ];

        for (input, expected) in test_cases {
            let video_id = extract_id(input);
            assert_eq!(video_id, expected);
        }
    }

    #[test]
    fn parse_xml_works() {
        let xml = r#"
<?xml version="1.0" encoding="utf-8" ?><transcript><text start="2.4" dur="2.49">Recently, my podcast team was in Australia</text><text start="4.89" dur="5">and my producer and close
friend here, Rob Mohr,</text><text start="10.53" dur="1.17">instructed all of us</text><text start="11.7" dur="1.8">to get rid of social media on our phones,</text><text start="13.5" dur="1.8">except one guy who would post</text><text start="15.3" dur="2.223">our weekly episodes announcements.</text><text start="18.57" dur="2.43">And it was pretty brutal at first.</text><text start="21" dur="1.8">And then coming back to social media</text><text start="22.8" dur="2.85">has actually turned out
to be more challenging.</text><text start="25.65" dur="0.833">Huh.</text><text start="26.483" dur="0.833">And you really experienced</text><text start="27.316" dur="1.484">the friction coming back the other way.</text><text start="28.8" dur="2.82">And then one experiences
the lack of friction,</text><text start="31.62" dur="1.08">and that&amp;#39;s where it gets scary.</text><text start="32.7" dur="4.47">It&amp;#39;s so interesting the way
that the brain can adapt,</text><text start="37.17" dur="3.21">the friction leaving something behind,</text><text start="40.38" dur="1.55">the friction coming back to it.</text><text start="42.93" dur="1.98">And I think for people listening to this,</text><text start="44.91" dur="2.7">I raise this because, I think, of course,</text><text start="47.61" dur="1.83">many people listening are, you know,</text><text start="49.44" dur="2.52">have work that they
really need to focus on.</text><text start="51.96" dur="2.4">They may be having
issues with productivity</text><text start="54.36" dur="1.47">and burnout, et cetera.</text><text start="55.83" dur="2.34">I think a lot of people use
the phone and social media</text><text start="58.17" dur="2.1">because it fills their life, you know?</text><text start="60.27" dur="1.5">It provides some enrichment</text><text start="61.77" dur="2.76">and they aren&amp;#39;t necessarily
committed to specific projects.</text><text start="64.53" dur="2.31">But I guess through the lens of the,</text><text start="66.84" dur="2.25">let&amp;#39;s just call it the
Cal Newportian lens,</text><text start="69.09" dur="3.06">one might argue that those
people almost certainly</text><text start="72.15" dur="4.05">have untapped creativity,
untapped resources within them</text><text start="76.2" dur="2.91">that they don&amp;#39;t yet know about</text><text start="79.11" dur="3.66">because they&amp;#39;re essentially
using that energy elsewhere.</text><text start="82.77" dur="1.83">Yeah, I mean, I think for a lot of people,</text><text start="84.6" dur="1.98">it&amp;#39;s papering over the void, right?</text><text start="86.58" dur="1.98">You have this void in your life</text><text start="88.56" dur="5">because there&amp;#39;s unmet
potential, unmet interest,</text><text start="93.677" dur="1.333">living in misalignment</text><text start="95.01" dur="1.41">with the things you care about, right?</text><text start="96.42" dur="1.98">I mean, a lot of people,
this is the classic sort</text><text start="98.4" dur="2.19">of catastrophe of life, right?</text><text start="100.59" dur="2.46">Social media, and before this,
it was other things, right,</text><text start="103.05" dur="3.06">there was other intoxicants or
other sorts of distractions.</text><text start="106.11" dur="2.94">It&amp;#39;s a way for some
people of, essentially,</text><text start="109.05" dur="2.88">putting a screen over
that like gaping void.</text><text start="111.93" dur="2.67">And it like, just makes it bearable enough</text><text start="114.6" dur="2.19">that you can kind of go on with life.</text><text start="116.79" dur="3.27">And so it is true, if you just
rip it out, you see the void.</text><text start="120.06" dur="1.26">And that&amp;#39;s really difficult, right?</text><text start="121.32" dur="3.21">I mean, &amp;#39;cause I did this
experiment for one of my books.</text><text start="124.53" dur="2.88">I ran an experiment with 1,600 people</text><text start="127.41" dur="3.27">and they all turned off all
their social media for 30 days.</text><text start="130.68" dur="1.29">30 days.
30 days, right?</text><text start="131.97" dur="1.44">These are young people, old people?</text><text start="133.41" dur="1.68">A whole mix, a whole mix, right?</text><text start="135.09" dur="1.23">So not just university students.</text><text start="136.32" dur="2.52">I recruited them from my
newsletter readership,</text><text start="138.84" dur="1.17">so they weren&amp;#39;t university students.</text><text start="140.01" dur="1.59">And it wasn&amp;#39;t formal
research, it was, you know,</text><text start="141.6" dur="1.35">I put out the call, right?</text><text start="142.95" dur="1.53">So this is not randomly sampled, right?</text><text start="144.48" dur="0.877">But I put out the call and I said,</text><text start="145.357" dur="2.123">&amp;quot;Here, I&amp;#39;m going to
walk you through this.&amp;quot;</text><text start="147.48" dur="1.47">And then I got a lot of information back.</text><text start="148.95" dur="2.22">So people reported back how it went.</text><text start="151.17" dur="2.94">And this was like, the
number one thing I heard was,</text><text start="154.11" dur="1.83">it&amp;#39;s really hard at first, right?</text><text start="155.94" dur="2.07">And so, who are the people that succeeded</text><text start="158.01" dur="1.89">for 30 days versus those who didn&amp;#39;t?</text><text start="159.9" dur="1.8">The ones who didn&amp;#39;t succeed,</text><text start="161.7" dur="2.13">tended to just try to white knuckle it,</text><text start="163.83" dur="2.52">just be like, &amp;quot;I don&amp;#39;t like how
much I&amp;#39;m using social media,</text><text start="166.35" dur="1.65">I&amp;#39;m just going to stop because it&amp;#39;s bad</text><text start="168" dur="0.96">and I don&amp;#39;t want to do a bad thing.</text><text start="168.96" dur="1.08">I&amp;#39;m just going to like, you know,</text><text start="170.04" dur="1.56">hold onto the table with white knuckles.&amp;quot;</text><text start="171.6" dur="1.59">They wouldn&amp;#39;t make it 30 days.</text><text start="173.19" dur="2.73">The people who did
succeed followed my advice</text><text start="175.92" dur="3.54">to incredibly, aggressively
pursue alternatives</text><text start="179.46" dur="0.9">in those 30 days.</text><text start="180.36" dur="2.64">So it&amp;#39;s like, go learn new
hobbies, join things right away,</text><text start="183" dur="2.49">get like really structured about your day,</text><text start="185.49" dur="2.4">get into exercise again,
learn how to knit again.</text><text start="187.89" dur="1.267">A lot of people said,</text><text start="189.157" dur="3.023">&amp;quot;Oh, I forgot how fun libraries were.</text><text start="192.18" dur="1.38">Like, you can go into
this building and like,</text><text start="193.56" dur="2.67">all the books are free and
you could just grab whatever.</text><text start="196.23" dur="1.32">And it&amp;#39;s okay if you don&amp;#39;t like the book</text><text start="197.55" dur="1.8">because you didn&amp;#39;t have to pay for it.</text><text start="199.35" dur="2.01">I&amp;#39;m going out with friends again.</text><text start="201.36" dur="2.16">Okay, every week I&amp;#39;m
going to have, you know,</text><text start="203.52" dur="1.59">we&amp;#39;re going to have
drinks with this person</text><text start="205.11" dur="1.38">and every Thursday morning I&amp;#39;m</text><text start="206.49" dur="1.59">going to go running with this person.&amp;quot;</text><text start="208.08" dur="1.56">The people who aggressively tried</text><text start="209.64" dur="3.18">to put in place a more
positive alternative</text><text start="212.82" dur="1.29">through self-reflection experimentation,</text><text start="214.11" dur="2.61">they lasted the 30 days and beyond, right?</text><text start="216.72" dur="1.5">And so then I came to realize like, oh,</text><text start="218.22" dur="3.3">I see what&amp;#39;s happening here
is you have these unmet needs.</text><text start="221.52" dur="2.01">These tools can give you sort of</text><text start="223.53" dur="1.528">a simulacrum of meeting them.</text><text start="225.058" dur="2.672">I&amp;#39;m a social being, I need
to be connected to people.</text><text start="227.73" dur="3.6">Well, I&amp;#39;m texting and like
doing comments on social media,</text><text start="231.33" dur="2.13">it sort of touches that a little bit,</text><text start="233.46" dur="1.89">just enough that you don&amp;#39;t
feel hopelessly lonely,</text><text start="235.35" dur="1.83">but it&amp;#39;s not really fulfilling that.</text><text start="237.18" dur="0.87">I have a need to, like,</text><text start="238.05" dur="2.73">see my intentions made manifest
concretely in the world,</text><text start="240.78" dur="1.14">humans want to do this.</text><text start="241.92" dur="2.04">Well, I&amp;#39;m, you know, posting these things</text><text start="243.96" dur="1.11">and people are responding,</text><text start="245.07" dur="2.79">it&amp;#39;s sort of this
simulacrum of real creation.</text><text start="247.86" dur="2.79">So it&amp;#39;s like kind of
satisfying that just enough</text><text start="250.65" dur="2.73">that it&amp;#39;s not just intolerable, right?</text><text start="253.38" dur="2.28">And so what happens is if you remove that,</text><text start="255.66" dur="2.16">you have to actually fill
those things the right way.</text><text start="257.82" dur="2.61">So now I&amp;#39;m not socializing
on social media,</text><text start="260.43" dur="2.07">but I&amp;#39;m going out of my
way to sacrifice time</text><text start="262.5" dur="1.77">and attention on behalf of other people.</text><text start="264.27" dur="2.31">I&amp;#39;m feeling the social
void in the right way,</text><text start="266.58" dur="2.04">now I don&amp;#39;t really feel
like I need to go back.</text><text start="268.62" dur="3.33">I&amp;#39;m actually making my
intentions manifest,</text><text start="271.95" dur="1.53">I&amp;#39;m learning skills and building things.</text><text start="273.48" dur="2.46">Now this sort of pseudo construction</text><text start="275.94" dur="2.16">and collective attention
economy of social media,</text><text start="278.1" dur="2.91">I&amp;#39;ll post this and you&amp;#39;ll
like it, I don&amp;#39;t like this,</text><text start="281.01" dur="1.92">I don&amp;#39;t need that anymore
to fill that void.</text><text start="282.93" dur="3.03">So it&amp;#39;s like you have
to fill the void first.</text><text start="285.96" dur="1.8">So, you know, five years
ago I wrote a book,</text><text start="287.76" dur="3.24">it was about reforming
this part of your life.</text><text start="291" dur="2.82">And a lot of the book had
nothing to do with technology,</text><text start="293.82" dur="3.87">but about how to actually just
rebuild parts of your life.</text><text start="297.69" dur="1.95">And on my podcast, honestly, like one</text><text start="299.64" dur="1.38">of the big topics we talk about,</text><text start="301.02" dur="2.46">which is crazy that I&amp;#39;m a
technologist and I write</text><text start="303.48" dur="2.85">about trying to find focus
in a distracted world,</text><text start="306.33" dur="1.98">is this thing we call the deep life,</text><text start="308.31" dur="3.99">which is just straight up
building a meaningful life 101.</text><text start="312.3" dur="2.55">And it&amp;#39;s like crazy that my
podcast is talking about it,</text><text start="314.85" dur="1.05">but on the other hand, it&amp;#39;s not,</text><text start="315.9" dur="1.62">because mine is the podcast people go to</text><text start="317.52" dur="1.41">when they&amp;#39;re fed up
with the digital world.</text><text start="318.93" dur="1.26">And it turns out if you don&amp;#39;t get</text><text start="320.19" dur="3.63">the analog world working right for you,</text><text start="323.82" dur="2.58">you need something to
avoid staring to that void,</text><text start="326.4" dur="1.8">and the digital world
will do that well enough.</text><text start="328.2" dur="2.67">It&amp;#39;s like just good enough
to keep life tolerable.</text><text start="330.87" dur="2.7">Thank you for tuning into the
Huberman Lab Clips channel.</text><text start="333.57" dur="2.13">If you enjoyed the clip
that you just viewed,</text><text start="335.7" dur="3.033">please check out the full
length episode by clicking here.</text></transcript>
        "#;
        let result = parse_xml(xml);
        assert!(!result.is_empty());
        assert_eq!(result[0].timestamp, 2400);
    }
}
