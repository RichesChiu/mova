use crate::error::ApiError;
use mova_application::client_playback::TICKS_PER_SECOND;
use mova_domain::{
    AudioTrack, Library, MediaFile, MediaItem, MediaSourceKind, Season, SubtitleFile,
};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum Kind {
    User = 1,
    Library = 2,
    Season = 3,
    Item = 4,
    File = 5,
    Root = 6,
}

pub fn external_id(kind: Kind, id: i64) -> String {
    format!("{:016x}{:016x}", kind as u64, id)
}
pub fn parse_id(value: &str) -> Result<(Kind, i64), ApiError> {
    if value.len() != 32 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ApiError::BadRequest("invalid resource id".into()));
    }
    let tag = u64::from_str_radix(&value[..16], 16)
        .map_err(|_| ApiError::BadRequest("invalid resource kind".into()))?;
    let kind = match tag {
        1 => Kind::User,
        2 => Kind::Library,
        3 => Kind::Season,
        4 => Kind::Item,
        5 => Kind::File,
        6 => Kind::Root,
        _ => return Err(ApiError::BadRequest("unknown resource kind".into())),
    };
    let id = i64::from_str_radix(&value[16..], 16)
        .map_err(|_| ApiError::BadRequest("invalid resource id".into()))?;
    if id <= 0 {
        return Err(ApiError::BadRequest("invalid resource id".into()));
    }
    Ok((kind, id))
}
pub fn require_id(value: &str, kind: Kind) -> Result<i64, ApiError> {
    let (actual, id) = parse_id(value)?;
    if actual != kind {
        return Err(ApiError::BadRequest("incorrect resource kind".into()));
    }
    Ok(id)
}
pub fn ticks(seconds: i32) -> i64 {
    i64::from(seconds) * TICKS_PER_SECOND
}
fn timestamp(t: time::OffsetDateTime) -> String {
    t.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}
fn image_tags(poster: bool, backdrop: bool, logo: bool, tag: &str) -> Value {
    let mut tags = json!({});
    if poster {
        tags["Primary"] = json!(tag);
    }
    if logo {
        tags["Logo"] = json!(tag);
    }
    json!({"ImageTags":tags,"BackdropImageTags":if backdrop{vec![tag]}else{Vec::new()}})
}
pub fn library(l: &Library, server: &str) -> Value {
    json!({"Id":external_id(Kind::Library,l.id),"ServerId":server,"Name":l.name,"Overview":l.description,
        "Type":"CollectionFolder","IsFolder":true,"CollectionType":null,"ParentId":external_id(Kind::Root,1),"ImageTags":{},"BackdropImageTags":[]})
}
pub fn season(s: &Season, server: &str) -> Value {
    let mut result = json!({"Id":external_id(Kind::Season,s.id),"ServerId":server,"Name":s.title.clone().unwrap_or_else(||format!("Season {}",s.season_number)),"Overview":s.overview,
        "Type":"Season","IsFolder":true,"IndexNumber":s.season_number,"SeriesId":external_id(Kind::Item,s.series_id),"ParentId":external_id(Kind::Item,s.series_id),"ChildCount":s.episode_count});
    let images = image_tags(
        s.poster_path.is_some(),
        s.backdrop_path.is_some(),
        false,
        &s.updated_at.unix_timestamp_nanos().to_string(),
    );
    result
        .as_object_mut()
        .unwrap()
        .extend(images.as_object().unwrap().clone());
    result
}

pub fn subtitle_indexes(audio: &[AudioTrack], subtitles: &[SubtitleFile]) -> Vec<i32> {
    let mut next = audio
        .iter()
        .map(|a| a.stream_index)
        .chain(subtitles.iter().filter_map(|s| s.stream_index))
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    subtitles
        .iter()
        .map(|s| {
            s.stream_index.unwrap_or_else(|| {
                let i = next;
                next = next.saturating_add(1);
                i
            })
        })
        .collect()
}

pub fn media_source(
    f: &MediaFile,
    audio: &[AudioTrack],
    subs: &[SubtitleFile],
    token: &str,
) -> Value {
    let item = external_id(Kind::Item, f.media_item_id);
    let source = external_id(Kind::File, f.id);
    let url =
        format!("/emby/Videos/{item}/stream?Static=true&MediaSourceId={source}&api_key={token}");
    let mut streams = Vec::new();
    if f.source_kind != MediaSourceKind::Strm {
        // The scanner does not persist the video stream index. Never invent one.
        streams.push(json!({"Type":"Video","Codec":f.video_codec,"Width":f.width,"Height":f.height,"BitRate":f.video_bitrate,"AverageFrameRate":f.video_frame_rate,"Profile":f.video_profile,"IsExternal":false}));
        for a in audio {
            streams.push(json!({"Type":"Audio","Index":a.stream_index,"Codec":a.audio_codec,"Language":a.language,"DisplayTitle":a.label,"Channels":a.channels,"ChannelLayout":a.channel_layout,"SampleRate":a.sample_rate,"BitRate":a.bitrate,"IsDefault":a.is_default,"IsExternal":false}));
        }
    }
    let indexes = subtitle_indexes(audio, subs);
    for (s, index) in subs.iter().zip(&indexes) {
        streams.push(json!({"Type":"Subtitle","Index":index,"Codec":"webvtt","Language":s.language,"DisplayTitle":s.label,
            "IsDefault":s.is_default,"IsForced":s.is_forced,"IsExternal":true,"SupportsExternalStream":true,"DeliveryMethod":"External",
            "DeliveryUrl":format!("/emby/Videos/{item}/{source}/Subtitles/{index}/Stream.vtt?api_key={token}")}));
    }
    let mut result = json!({"Id":source,"ItemId":item,"Name":f.video_title.clone().unwrap_or_else(||std::path::Path::new(&f.file_path).file_name().map(|s|s.to_string_lossy().into_owned()).unwrap_or_else(||"Video".into())),
        "Path":url,"Protocol":"Http","Type":"Default","IsRemote":f.source_kind==MediaSourceKind::Strm,
        "SupportsDirectPlay":true,"SupportsDirectStream":true,"SupportsTranscoding":false,"RequiresOpening":false,"RequiresClosing":false,"IsInfiniteStream":false,
        "DirectStreamUrl":url,"MediaStreams":streams,"DefaultAudioStreamIndex":audio.iter().find(|a|a.is_default).or_else(||audio.first()).map(|a|a.stream_index),
        "DefaultSubtitleStreamIndex":subs.iter().position(|s|s.is_default||s.is_forced).map(|i|indexes[i])});
    if let Some(c) = &f.container {
        result["Container"] = json!(c);
    }
    if f.source_kind != MediaSourceKind::Strm {
        result["Size"] = json!(f.file_size);
    }
    if let Some(d) = f.duration_seconds {
        result["RunTimeTicks"] = json!(ticks(d));
    }
    if let Some(b) = f.bitrate {
        result["Bitrate"] = json!(b);
    }
    result
}

pub fn item(i: &MediaItem, details: &mova_db::CatalogDetails, server: &str, token: &str) -> Value {
    let files = details
        .files
        .get(&i.id)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let progress = details.progress.get(&i.id);
    let sources: Vec<_> = files
        .iter()
        .map(|f| {
            media_source(
                f,
                details
                    .audio
                    .get(&f.id)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                details
                    .subtitles
                    .get(&f.id)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                token,
            )
        })
        .collect();
    let kind = match i.media_type.as_str() {
        "series" => "Series",
        "episode" => "Episode",
        _ => "Movie",
    };
    let mut providers = json!({});
    if let (Some(provider), Some(id)) = (&i.metadata_provider, &i.metadata_provider_item_id) {
        providers[if provider == "tmdb" { "Tmdb" } else { provider }] = json!(id);
    }
    let duration = files.iter().find_map(|f| f.duration_seconds);
    let mut result = json!({"Id":external_id(Kind::Item,i.id),"ServerId":server,"Name":i.title,"OriginalTitle":i.original_title,"SortName":i.sort_title.as_ref().unwrap_or(&i.title),
        "Type":kind,"MediaType":"Video","IsFolder":i.media_type=="series","ParentId":external_id(Kind::Library,i.library_id),
        "Overview":i.overview,"ProductionYear":i.year,"OfficialRating":i.content_rating,"ProviderIds":providers,"DateCreated":timestamp(i.created_at),
        "Genres":i.genres.as_deref().map(|s|s.split([',','/','|']).map(str::trim).filter(|s|!s.is_empty()).collect::<Vec<_>>()).unwrap_or_default(),
        "MediaSources":sources,"MediaSourceCount":files.len(),"LocationType":"FileSystem",
        "UserData":{"PlaybackPositionTicks":progress.map_or(0,|p|ticks(p.position_seconds)),"Played":progress.is_some_and(|p|p.is_finished),"IsFavorite":false,"Key":external_id(Kind::Item,i.id),"LastPlayedDate":progress.map(|p|timestamp(p.last_watched_at))}});
    if let Some(d) = duration {
        result["RunTimeTicks"] = json!(ticks(d));
    }
    if let Some(date) = i.premiere_date {
        result["PremiereDate"] = json!(format!("{date}T00:00:00Z"));
    }
    if let Some(r) = i
        .ratings
        .iter()
        .find(|r| r.source == "tmdb" && r.scale > 0.0)
    {
        result["CommunityRating"] = json!(r.score * 10.0 / r.scale);
    }
    let images = image_tags(
        i.poster_path.is_some(),
        i.backdrop_path.is_some(),
        i.logo_path.is_some(),
        &i.updated_at.unix_timestamp_nanos().to_string(),
    );
    result
        .as_object_mut()
        .unwrap()
        .extend(images.as_object().unwrap().clone());
    if let Some(e) = details.episodes.get(&i.id) {
        result["ParentId"] = json!(external_id(Kind::Season, e.season_id));
        result["SeasonId"] = json!(external_id(Kind::Season, e.season_id));
        result["SeriesId"] = json!(external_id(Kind::Item, e.series_id));
        result["SeriesName"] = json!(e.series_name);
        result["IndexNumber"] = json!(e.episode_number);
        result["ParentIndexNumber"] = json!(e.season_number);
    }
    result
}
