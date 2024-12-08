use crate::channel::ListItem;
use crate::OPTIONS;
use crate::{
    api::{Format, VideoInfo},
    app::SelectionList,
};
use std::fmt::Display;

#[derive(Default)]
pub struct Formats {
    pub video_formats: SelectionList<Format>,
    pub audio_formats: SelectionList<Format>,
    pub formats: SelectionList<Format>,
    pub captions: SelectionList<Format>,
    pub selected_tab: usize,
    pub use_adaptive_streams: bool,
}

impl Formats {
    pub fn new(video_info: VideoInfo) -> Self {
        let mut formats = Formats {
            video_formats: SelectionList::new(video_info.video_formats),
            audio_formats: SelectionList::new(video_info.audio_formats),
            formats: SelectionList::new(video_info.format_streams),
            captions: SelectionList::new(video_info.captions),
            selected_tab: 0,
            use_adaptive_streams: OPTIONS.prefer_dash_formats,
        };

        formats.set_preferred();

        formats
    }

    fn set_preferred(&mut self) {
        let mut video_idx = None;

        for (idx, video_format) in self.video_formats.items.iter().enumerate() {
            let (quality, _) = video_format.item.get_quality().split_once('p').unwrap();

            let quality: u16 = quality.parse().unwrap();

            if quality == OPTIONS.video_quality {
                video_idx = Some(idx);
                break;
            }
        }

        if video_idx.is_none() {
            video_idx = Some(0);
        }

        self.video_formats.items[video_idx.unwrap()].selected = true;

        let mut audio_idx = None;

        for (idx, audio_format) in self.audio_formats.items.iter().enumerate() {
            if matches!(&audio_format.item, Format::Audio { language,.. } if language.as_ref().is_some_and(|(_, is_default)| *is_default))
            {
                audio_idx = Some(idx);
                break;
            }
        }

        if audio_idx.is_none() {
            audio_idx = Some(0);
        }

        self.audio_formats.items[audio_idx.unwrap()].selected = true;

        if let Some(item) = self.formats.items.first_mut() {
            item.selected = true;
        }

        for language in &crate::OPTIONS.subtitle_languages {
            if let Some(caption) = self
                .captions
                .items
                .iter_mut()
                .find(|caption| caption.item.id() == language)
            {
                caption.selected = true;
            }
        }

        for caption in &mut self.captions.items {
            if crate::OPTIONS
                .subtitle_languages
                .iter()
                .any(|language| *language == caption.item.id() || matches!(caption.item.id().split_once('-'), Some((lang, _)) if lang == *language))
            {
                caption.selected = true;
            }
        }
    }

    pub fn switch_format_type(&mut self) {
        self.use_adaptive_streams = !self.use_adaptive_streams;
        self.selected_tab = 0;
    }

    pub fn get_mut_selected_tab(&mut self) -> &mut SelectionList<Format> {
        match self.selected_tab {
            0 if self.use_adaptive_streams => &mut self.video_formats,
            0 => &mut self.formats,
            1 => &mut self.audio_formats,
            2 => &mut self.captions,
            _ => panic!(),
        }
    }

    pub fn next_tab(&mut self) {
        self.selected_tab = (self.selected_tab + 1) % 3;

        if !self.use_adaptive_streams && self.selected_tab == 1 {
            self.next_tab();
        }

        if self.get_mut_selected_tab().items.is_empty() {
            self.next_tab();
        }
    }

    pub fn previous_tab(&mut self) {
        self.selected_tab = if self.selected_tab == 0 {
            2
        } else {
            self.selected_tab - 1
        };

        if !self.use_adaptive_streams && self.selected_tab == 1 {
            self.previous_tab();
        }

        if self.get_mut_selected_tab().items.is_empty() {
            self.previous_tab();
        }
    }
}

impl Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Format::Video {
                quality,
                fps,
                r#type,
                ..
            } => write!(f, "{quality} @ {fps} fps, {type}"),
            Format::Audio {
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
            Format::Stream {
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
            Format::Caption { label, .. } => write!(f, "{label}"),
        }
    }
}
