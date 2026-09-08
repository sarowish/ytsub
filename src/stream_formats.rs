use crate::CONFIG;
use crate::api::Chapters;
use crate::video::PlaybackSpec;
use crate::{
    api::{Format, VideoInfo},
    list::{ListItem, SelectionList},
};
use std::fmt::Display;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FormatList {
    Video,
    Audio,
    Combined,
    Caption,
}

#[derive(Default)]
pub struct Formats {
    pub spec: PlaybackSpec,
    pub video_formats: SelectionList<Format>,
    pub audio_formats: SelectionList<Format>,
    pub formats: SelectionList<Format>,
    pub captions: SelectionList<Format>,
    pub chapters: Option<Chapters>,
    pub selected_tab: usize,
    pub use_adaptive_streams: bool,
}

impl Formats {
    pub fn new(spec: PlaybackSpec, video_info: VideoInfo) -> Self {
        let mut formats = Self {
            spec,
            video_formats: SelectionList::new(video_info.video_formats),
            audio_formats: SelectionList::new(video_info.audio_formats),
            formats: SelectionList::new(video_info.format_streams),
            captions: SelectionList::new(video_info.captions),
            chapters: video_info.chapters,
            selected_tab: 0,
            use_adaptive_streams: CONFIG.prefer_dash_formats,
        };

        formats.set_preferred();

        formats
    }

    fn set_preferred(&mut self) {
        let mut video_idx = None;

        for (idx, format) in self.video_formats.items().iter().enumerate() {
            if let Some(preferred_codec) = &CONFIG.preferred_video_codec {
                if CONFIG.video_quality == format.get_quality() {
                    video_idx = Some(idx);
                }

                if *preferred_codec == format.get_codec() {
                    match video_idx {
                        Some(video_idx) if idx == video_idx => break,
                        None => video_idx = Some(idx),
                        _ => (),
                    }
                }
            } else if CONFIG.video_quality == format.get_quality() {
                video_idx = Some(idx);
                break;
            }
        }

        self.video_formats
            .update_item_at(video_idx.unwrap_or_default(), |item| item.selected = true);

        let mut audio_idx = None;

        for (idx, format) in self.audio_formats.items().iter().enumerate() {
            if matches!(&format.item, Format::Audio { language,.. } if language.as_ref().is_some_and(|(_, is_default)| *is_default))
            {
                audio_idx = Some(idx);

                if CONFIG.preferred_audio_codec.is_none() {
                    break;
                }
            }

            if CONFIG
                .preferred_audio_codec
                .as_ref()
                .is_some_and(|preferred| *preferred == format.get_codec())
            {
                match audio_idx {
                    Some(audio_idx) if idx == audio_idx => break,
                    None => audio_idx = Some(idx),
                    _ => (),
                }
            }
        }

        self.audio_formats
            .update_item_at(audio_idx.unwrap_or_default(), |item| item.selected = true);

        self.formats.update_item_at(0, |item| item.selected = true);

        for language in &CONFIG.subtitle_languages {
            self.captions
                .update_item(language, |item| item.selected = true);
        }

        self.captions.update_each(|caption|
            if CONFIG
                .subtitle_languages
                .iter()
                .any(|language| *language == caption.item.id() || matches!(caption.item.id().split_once('-'), Some((lang, _)) if lang == *language))
            {
                caption.selected = true;
            }
        );
    }

    pub const fn switch_format_type(&mut self) {
        self.use_adaptive_streams = !self.use_adaptive_streams;
        self.selected_tab = 0;
    }

    pub const fn selected_list(&self) -> FormatList {
        match self.selected_tab {
            0 if self.use_adaptive_streams => FormatList::Video,
            0 => FormatList::Combined,
            1 => FormatList::Audio,
            2 => FormatList::Caption,
            _ => unreachable!(),
        }
    }

    pub fn list(&self, target: FormatList) -> &SelectionList<Format> {
        match target {
            FormatList::Video => &self.video_formats,
            FormatList::Audio => &self.audio_formats,
            FormatList::Combined => &self.formats,
            FormatList::Caption => &self.captions,
        }
    }

    pub fn list_mut(&mut self, target: FormatList) -> &mut SelectionList<Format> {
        match target {
            FormatList::Video => &mut self.video_formats,
            FormatList::Audio => &mut self.audio_formats,
            FormatList::Combined => &mut self.formats,
            FormatList::Caption => &mut self.captions,
        }
    }

    pub fn get_mut_selected_tab(&mut self) -> &mut SelectionList<Format> {
        self.list_mut(self.selected_list())
    }

    pub fn next_tab(&mut self) {
        for _ in 0..3 {
            self.selected_tab = (self.selected_tab + 1) % 3;

            if (self.selected_tab != 2 || !self.get_mut_selected_tab().items().is_empty())
                && (self.use_adaptive_streams || self.selected_tab != 1)
            {
                break;
            }
        }
    }

    pub fn previous_tab(&mut self) {
        for _ in 0..3 {
            self.selected_tab = if self.selected_tab == 0 {
                2
            } else {
                self.selected_tab - 1
            };

            if (self.selected_tab != 2 || !self.get_mut_selected_tab().items().is_empty())
                && (self.use_adaptive_streams || self.selected_tab != 1)
            {
                break;
            }
        }
    }

    pub fn get_selected_video_url(&self) -> Option<(&str, Option<&str>)> {
        Some(if self.use_adaptive_streams {
            (
                self.video_formats.get_selected_item()?.get_url(),
                Some(self.audio_formats.get_selected_item()?.get_url()),
            )
        } else {
            (self.formats.get_selected_item()?.get_url(), None)
        })
    }

    pub fn get_selected_audio_url(&self) -> Option<&str> {
        self.audio_formats
            .get_selected_item()
            .or_else(|| self.formats.get_selected_item())
            .map(Format::get_url)
    }
}

impl Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Video {
                quality,
                fps,
                r#type,
                ..
            } => write!(f, "{quality} @ {fps} fps, {type}"),
            Self::Audio {
                language,
                bitrate,
                r#type,
                ..
            } => write!(
                f,
                "{}{}, {}",
                language
                    .as_ref()
                    .map_or(String::new(), |(language, _)| format!("{language}, ")),
                bitrate,
                r#type
            ),
            Self::Stream {
                quality,
                fps,
                bitrate,
                r#type,
                ..
            } => write!(
                f,
                "{} @ {} fps, {}, {}",
                quality,
                fps,
                bitrate.clone().unwrap_or_default(),
                r#type
            ),
            Self::Caption { label, .. } => write!(f, "{label}"),
        }
    }
}
