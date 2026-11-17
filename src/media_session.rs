//! Music widget source: Windows' system media sessions (the ones the media
//! flyout and the media keys drive).
//!
//! Runs on its own MTA thread. Session events and media key presses only wake
//! it; it then re-reads the system's current session and decides whether the
//! change is worth a widget:
//!
//! - A media key (play/pause, next, previous, stop) opens a short window. The
//!   change it causes (new state, new track) shows the widget; a player that
//!   reacts slowly, or a "previous" that only restarts the track, still shows
//!   it after [`KEY_FALLBACK`]. Changes inside the window update it in place.
//! - A track change nobody asked for, in the same app, while it was playing
//!   and close to the end of the old track, is "Up next".
//!
//! Track changes from anything else (clicking next inside the player) and
//! play/pause from the player's own buttons stay quiet.

use crate::volume_osd::MediaAction;
use gpui::{Image, ImageFormat};
use once_cell::sync::Lazy;
use std::{
    sync::{Arc, Mutex, Once},
    thread,
    time::{Duration, Instant},
};
use windows::{
    Foundation::{EventRegistrationToken, TypedEventHandler},
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
    },
    Storage::Streams::DataReader,
    Win32::System::{
        SystemInformation::GetSystemTimeAsFileTime,
        WinRT::{RO_INIT_MULTITHREADED, RoInitialize},
    },
};

/// Cover edge in pixels: the widget's cover at up to ~350% size, no upscale.
const COVER_PX: u32 = 192;
/// Thumbnails larger than this are not worth reading.
const MAX_THUMBNAIL_BYTES: u64 = 8 * 1024 * 1024;
/// Players fire several events per change; read once they settle.
const SETTLE: Duration = Duration::from_millis(80);
/// Changes this long after a media key still belong to that key.
const KEY_WINDOW: Duration = Duration::from_millis(2_500);
/// Show the widget this long after a key even when the player has not
/// reported a change yet.
const KEY_FALLBACK: Duration = Duration::from_millis(450);
/// Late cover art for a widget shown this recently replaces the placeholder.
const LATE_ART: Duration = Duration::from_secs(4);
/// A track that ends within this much of its length counts as finished.
/// In 100 ns units, like the session timeline.
const NEAR_END: i64 = 12 * 10_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKey {
    PlayPause,
    Next,
    Previous,
    Stop,
}

#[derive(Debug)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    /// Square, center-cropped album art. `None` when the player has none (yet).
    pub cover: Option<Arc<Image>>,
}

enum Wake {
    Key(MediaKey, Instant),
    Sessions,
    Changed,
}

static WAKE: Lazy<Mutex<Option<flume::Sender<Wake>>>> = Lazy::new(|| Mutex::new(None));
/// The last read track and whether it plays, for the settings preview.
static LATEST: Lazy<Mutex<Option<(Arc<NowPlaying>, bool)>>> = Lazy::new(|| Mutex::new(None));
static START: Once = Once::new();

/// Start watching media sessions. Later calls are free.
pub fn start() {
    START.call_once(|| {
        let (wake, woken) = flume::unbounded();
        *WAKE.lock().unwrap() = Some(wake.clone());
        let spawned = thread::Builder::new()
            .name("silence-media-session".into())
            .spawn(move || {
                if let Err(error) = watch(wake, woken) {
                    eprintln!("media session watcher: {error:#}");
                }
            });
        if let Err(error) = spawned {
            eprintln!("media session watcher: {error:#}");
        }
    });
}

/// A media key went down. Called from the keyboard hook: never blocks.
pub fn media_key(key: MediaKey) {
    if let Ok(wake) = WAKE.try_lock()
        && let Some(wake) = wake.as_ref()
    {
        let _ = wake.send(Wake::Key(key, Instant::now()));
    }
}

/// The current track and whether it plays, as last read. For previews.
pub fn latest() -> Option<(Arc<NowPlaying>, bool)> {
    start();
    LATEST.lock().unwrap().clone()
}

/// The track that is playing right now, for the volume OSD's strip.
pub fn playing_now() -> Option<Arc<NowPlaying>> {
    latest().and_then(|(track, playing)| playing.then_some(track))
}

/// What the watcher knows about the current session.
#[derive(Clone)]
struct Snapshot {
    app: String,
    /// Title and artist: one track.
    key: String,
    status: PlaybackStatus,
    track: Arc<NowPlaying>,
    /// Timeline in 100 ns units; `end` is 0 when the player does not say.
    position: i64,
    end: i64,
    /// System time (FILETIME) the position was taken at; 0 when unknown.
    updated: i64,
}

impl Snapshot {
    fn playing(&self) -> bool {
        self.status == PlaybackStatus::Playing
    }

    /// Where the track was at system time `now`, with playback running on.
    fn position_at(&self, now: i64) -> i64 {
        if self.playing() && self.updated > 0 && now > self.updated {
            self.position + (now - self.updated)
        } else {
            self.position
        }
    }
}

struct PendingKey {
    key: MediaKey,
    at: Instant,
    before: Option<(String, bool)>,
    shown: bool,
}

/// The widget last shown: when, why, for which track and with which art.
struct Shown {
    at: Instant,
    action: MediaAction,
    key: String,
    playing: bool,
    has_cover: bool,
}

struct Subscription {
    session: Session,
    playback: EventRegistrationToken,
    properties: EventRegistrationToken,
}

/// Cover of the last track, so play/pause events do not decode it again.
struct CoverCache {
    key: String,
    cover: Option<Arc<Image>>,
}

fn watch(wake: flume::Sender<Wake>, woken: flume::Receiver<Wake>) -> windows::core::Result<()> {
    unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
    let manager = Manager::RequestAsync()?.get()?;

    let sessions_wake = wake.clone();
    manager.SessionsChanged(&TypedEventHandler::new(move |_, _| {
        let _ = sessions_wake.send(Wake::Sessions);
        Ok(())
    }))?;
    let current_wake = wake.clone();
    manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
        let _ = current_wake.send(Wake::Changed);
        Ok(())
    }))?;

    let mut subscriptions = subscribe(&manager, &wake);
    let mut cache = CoverCache {
        key: String::new(),
        cover: None,
    };
    let mut current = read_current(&manager, &mut cache);
    publish(current.as_ref());
    // The last time each track was seen: its timeline tells whether it ran out.
    let mut previous_track = current.clone();
    let mut pending: Option<PendingKey> = None;
    let mut shown: Option<Shown> = None;

    loop {
        let wait = pending.as_ref().map(|pending| {
            let deadline = if pending.shown {
                pending.at + KEY_WINDOW
            } else {
                pending.at + KEY_FALLBACK
            };
            deadline.saturating_duration_since(Instant::now())
        });
        let first = match wait {
            Some(wait) => match woken.recv_timeout(wait) {
                Ok(wake) => Some(wake),
                Err(flume::RecvTimeoutError::Timeout) => None,
                Err(flume::RecvTimeoutError::Disconnected) => break,
            },
            None => match woken.recv() {
                Ok(wake) => Some(wake),
                Err(_) => break,
            },
        };
        if first.is_some() {
            thread::sleep(SETTLE);
        }
        let mut resubscribe = false;
        for wake in first.into_iter().chain(woken.try_iter()) {
            match wake {
                Wake::Key(key, at) => {
                    pending = Some(PendingKey {
                        key,
                        at,
                        before: current
                            .as_ref()
                            .map(|snapshot| (snapshot.key.clone(), snapshot.playing())),
                        shown: false,
                    });
                }
                Wake::Sessions => resubscribe = true,
                Wake::Changed => {}
            }
        }
        if resubscribe {
            unsubscribe(&mut subscriptions);
            subscriptions = subscribe(&manager, &wake);
        }

        let next = read_current(&manager, &mut cache);
        publish(next.as_ref());
        let now = Instant::now();
        if pending
            .as_ref()
            .is_some_and(|pending| now.duration_since(pending.at) >= KEY_WINDOW)
        {
            pending = None;
        }

        if let Some(next) = next.as_ref() {
            if let Some(key) = pending.as_mut() {
                if let Some(action) = key_action(key, next, shown.as_ref(), now) {
                    key.shown = true;
                    shown = Some(show(next, action));
                }
            } else if let Some(action) = unasked_action(previous_track.as_ref(), next, shown.as_ref(), now)
            {
                shown = Some(show(next, action));
            }
            // A player switching tracks reports "changing" for the old one
            // first; keep its last playing timeline for the next comparison.
            let same_track = previous_track
                .as_ref()
                .is_some_and(|previous| previous.key == next.key && previous.app == next.app);
            if !(same_track && next.status == PlaybackStatus::Changing) {
                previous_track = Some(next.clone());
            }
        }
        current = next;
    }
    Ok(())
}

/// What a pending media key shows for `next`, if anything new.
fn key_action(
    pending: &PendingKey,
    next: &Snapshot,
    shown: Option<&Shown>,
    now: Instant,
) -> Option<MediaAction> {
    let (before_key, before_playing) = pending
        .before
        .clone()
        .unwrap_or_else(|| (String::new(), false));
    let due = now.duration_since(pending.at) >= KEY_FALLBACK;
    let action = match pending.key {
        MediaKey::PlayPause | MediaKey::Stop => {
            let changed = next.playing() != before_playing;
            if !changed && !due {
                return None;
            }
            // Before the player confirms, assume the key did what it says.
            let playing = if changed || pending.shown {
                next.playing()
            } else {
                pending.key == MediaKey::PlayPause && !before_playing
            };
            if playing {
                MediaAction::Play
            } else {
                MediaAction::Pause
            }
        }
        MediaKey::Next | MediaKey::Previous => {
            if next.key == before_key && !due {
                return None;
            }
            if pending.key == MediaKey::Next {
                MediaAction::Next
            } else {
                MediaAction::Previous
            }
        }
    };
    let fresh = match shown {
        Some(shown) if pending.shown => {
            shown.action != action
                || shown.key != next.key
                || shown.playing != next.playing()
                || (!shown.has_cover && next.track.cover.is_some())
        }
        _ => true,
    };
    fresh.then_some(action)
}

/// "Up next", or late art for the widget on screen; `None` otherwise.
fn unasked_action(
    previous: Option<&Snapshot>,
    next: &Snapshot,
    shown: Option<&Shown>,
    now: Instant,
) -> Option<MediaAction> {
    if let Some(shown) = shown
        && now.duration_since(shown.at) < LATE_ART
        && shown.key == next.key
        && !shown.has_cover
        && next.track.cover.is_some()
    {
        return Some(shown.action);
    }
    let previous = previous?;
    let moved_on = previous.app == next.app
        && previous.key != next.key
        && previous.playing()
        && matches!(next.status, PlaybackStatus::Playing | PlaybackStatus::Changing);
    if !moved_on {
        return None;
    }
    // Without a timeline there is no telling a skip in the player from the
    // end of a song; the playlist moving on is the likelier one.
    let finished = previous.end <= 0 || previous.position_at(system_time()) >= previous.end - NEAR_END;
    finished.then_some(MediaAction::UpNext)
}

fn show(next: &Snapshot, action: MediaAction) -> Shown {
    crate::show_music_osd(next.track.clone(), action);
    Shown {
        at: Instant::now(),
        action,
        key: next.key.clone(),
        playing: next.playing(),
        has_cover: next.track.cover.is_some(),
    }
}

fn publish(snapshot: Option<&Snapshot>) {
    let next = snapshot.map(|snapshot| (snapshot.track.clone(), snapshot.playing()));
    let changed = {
        let mut latest = LATEST.lock().unwrap();
        let changed = match (latest.as_ref(), next.as_ref()) {
            (Some((a, a_playing)), Some((b, b_playing))) => {
                a_playing != b_playing
                    || a.title != b.title
                    || a.artist != b.artist
                    || a.cover.is_some() != b.cover.is_some()
            }
            (None, None) => false,
            _ => true,
        };
        *latest = next;
        changed
    };
    if changed {
        crate::volume_osd::media_changed();
    }
}

fn subscribe(manager: &Manager, wake: &flume::Sender<Wake>) -> Vec<Subscription> {
    let Ok(sessions) = manager.GetSessions() else {
        return Vec::new();
    };
    sessions
        .into_iter()
        .filter_map(|session| {
            let playback_wake = wake.clone();
            let playback = session
                .PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                    let _ = playback_wake.send(Wake::Changed);
                    Ok(())
                }))
                .ok()?;
            let properties_wake = wake.clone();
            let properties = session
                .MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                    let _ = properties_wake.send(Wake::Changed);
                    Ok(())
                }))
                .ok()?;
            Some(Subscription {
                session,
                playback,
                properties,
            })
        })
        .collect()
}

fn unsubscribe(subscriptions: &mut Vec<Subscription>) {
    for subscription in subscriptions.drain(..) {
        let _ = subscription
            .session
            .RemovePlaybackInfoChanged(subscription.playback);
        let _ = subscription
            .session
            .RemoveMediaPropertiesChanged(subscription.properties);
    }
}

/// The session media keys control: the system's current one, or else any
/// that plays.
fn current_session(manager: &Manager) -> Option<Session> {
    if let Ok(session) = manager.GetCurrentSession() {
        return Some(session);
    }
    manager.GetSessions().ok()?.into_iter().find(|session| {
        session
            .GetPlaybackInfo()
            .and_then(|info| info.PlaybackStatus())
            .is_ok_and(|status| status == PlaybackStatus::Playing)
    })
}

fn read_current(manager: &Manager, cache: &mut CoverCache) -> Option<Snapshot> {
    let session = current_session(manager)?;
    let status = session
        .GetPlaybackInfo()
        .and_then(|info| info.PlaybackStatus())
        .unwrap_or(PlaybackStatus::Closed);
    let properties = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let clean = |text: windows::core::Result<windows::core::HSTRING>| {
        text.map(|text| {
            text.to_string_lossy()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
    };
    let mut title = clean(properties.Title());
    let mut artist = clean(properties.Artist());
    if artist.is_empty() {
        artist = clean(properties.AlbumArtist());
    }
    if title.is_empty() {
        if artist.is_empty() {
            return None;
        }
        title = std::mem::take(&mut artist);
    }
    let app = session
        .SourceAppUserModelId()
        .map(|id| id.to_string_lossy())
        .unwrap_or_default();
    let key = format!("{title}\u{1}{artist}");
    let cover_key = format!("{app}\u{1}{key}");
    // A player often publishes the art a moment after the title: keep asking
    // until it arrives.
    if cache.key != cover_key || cache.cover.is_none() {
        cache.cover = properties
            .Thumbnail()
            .ok()
            .and_then(|thumbnail| read_cover(&thumbnail));
        cache.key = cover_key;
    }
    let (position, end, updated) = session
        .GetTimelineProperties()
        .map(|timeline| {
            (
                timeline.Position().map(|span| span.Duration).unwrap_or(0),
                timeline.EndTime().map(|span| span.Duration).unwrap_or(0),
                timeline
                    .LastUpdatedTime()
                    .map(|time| time.UniversalTime)
                    .unwrap_or(0),
            )
        })
        .unwrap_or((0, 0, 0));
    Some(Snapshot {
        app,
        key,
        status,
        track: Arc::new(NowPlaying {
            title,
            artist,
            cover: cache.cover.clone(),
        }),
        position,
        end,
        updated,
    })
}

/// System time as FILETIME ticks, the clock `LastUpdatedTime` uses.
fn system_time() -> i64 {
    let time = unsafe { GetSystemTimeAsFileTime() };
    (i64::from(time.dwHighDateTime) << 32) | i64::from(time.dwLowDateTime)
}

fn read_cover(
    thumbnail: &windows::Storage::Streams::IRandomAccessStreamReference,
) -> Option<Arc<Image>> {
    let stream = thumbnail.OpenReadAsync().ok()?.get().ok()?;
    let size = stream.Size().ok()?;
    if size == 0 || size > MAX_THUMBNAIL_BYTES {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).ok()?).ok()?;
    let loaded = reader.LoadAsync(size as u32).ok()?.get().ok()?;
    let mut bytes = vec![0u8; loaded as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    let decoded = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (width, height) = decoded.dimensions();
    let edge = width.min(height);
    if edge == 0 {
        return None;
    }
    // Video players hand out 16:9 frames; the widget's cover is square.
    let square = image::imageops::crop_imm(
        &decoded,
        (width - edge) / 2,
        (height - edge) / 2,
        edge,
        edge,
    )
    .to_image();
    let target = COVER_PX.min(edge);
    let square = if target == edge {
        square
    } else {
        image::imageops::resize(&square, target, target, image::imageops::FilterType::CatmullRom)
    };
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(square)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(Arc::new(Image::from_bytes(ImageFormat::Png, png)))
}
