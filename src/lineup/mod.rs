use crate::api;
use image::{ImageFormat, RgbaImage};

// Including the bytes here can be argued. On one hand it makes the bundling of the whole
// application just so much easier and reduces the runtime shenanigans that can occur
// due to bad filesystem reads. On the other hand, we are totally at the mercy of the OS
// here to eject these from memory when appropriate. That is, these are the default
// images that show up when the initial photos are loading, so at some point these
// are going to be totally unnecessary (until they are again). At that point we could
// let these assets fall out of scope and let their destructors runs which gives us
// fine grained control of when these get dumped out of memory. Now, the OS WILL evict
// these if it deems it necessary, but it doesn't have as such deep insight into how these
// buffers are being used so it can't be as intelligent.
static MLB_LOGO_LARGE_BYTES: &[u8] = include_bytes!("../../assets/mlb_logo_large.jpg");
static MLB_LOGO_SMALL_BYTES: &[u8] = include_bytes!("../../assets/mlb_logo_small.jpg");

lazy_static! {
    // Feel that unwrapping in lazy statics is reasonable. These are OUR images that we
    // baked into the binary so if they fail to parse a runtime then...yeah, that
    // seems like a stop-the-world moment.
    static ref MLB_LOGO_LARGE: RgbaImage =
        image::load_from_memory_with_format(MLB_LOGO_LARGE_BYTES, ImageFormat::JPEG)
            .unwrap()
            .into_rgba();
    static ref MLB_LOGO_SMALL: RgbaImage =
        image::load_from_memory_with_format(MLB_LOGO_SMALL_BYTES, ImageFormat::JPEG)
            .unwrap()
            .into_rgba();
}

/// A Schedule is a scrollable listing of games from a particular date
pub struct Schedule {
    pub games: Vec<Game>,
    cursor: usize,
}

impl Schedule {
    const PAGE_SIZE: usize = 5;

    pub fn left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn right(&mut self) {
        if self.cursor < self.games.len() - 2 {
            self.cursor += 1;
        }
    }

    /// Queries whether or not there is an additional page of content to the right
    /// of the current page.
    pub fn has_more(&self) -> bool {
        self.cursor < self.games.len() - Self::PAGE_SIZE
    }

    /// Queries whether or not there is an additional page of content to the left
    /// of the current page.
    pub fn has_less(&self) -> bool {
        self.cursor > Self::PAGE_SIZE - 1
    }

    /// Returns the list of game snippets for the current page. Each page has five games on it.
    ///
    /// E.G. If, there are are 14 games and we are focusing on game index 7, then this function will
    /// return games indices 5, 6, 7, 8, and 9 with 7 being the Snippet::Large variant.
    pub fn page(&mut self) -> Vec<Snippet> {
        let page = self.cursor / Self::PAGE_SIZE;
        // The left most snippet of this page.
        let left = page * Self::PAGE_SIZE;
        // The right end of the page can fall off if the map if we're on the last page.
        let right = match left + Self::PAGE_SIZE {
            right if right < self.games.len() - 1 => right,
            _ => self.games.len() - 1,
        };
        // The cursor may be 7, but the focus of this page is index 2.
        let page_focus = self.cursor % Self::PAGE_SIZE;
        // Sorry the extra parenthesis here, rustc thought that we were returning a &mut rather
        // than accessing self.games as a &mut.
        (&mut self.games)[left..right]
            .iter_mut()
            .enumerate()
            .map(|(index, game)| {
                if index == page_focus {
                    // If the underlying resource hasn't come in over the network yet, then this
                    // is the point where we decide to default to the appropriate size of the MLB logo.
                    Snippet::Large(
                        game.large.get().unwrap_or(&*MLB_LOGO_LARGE),
                        game.headline.as_str(),
                        game.subhead.as_str(),
                    )
                } else {
                    Snippet::Small(game.small.get().unwrap_or(&*MLB_LOGO_SMALL))
                }
            })
            .collect::<Vec<Snippet>>()
    }
}

impl From<api::Schedule> for Schedule {
    fn from(mut schedule: api::Schedule) -> Self {
        let mut games = vec![];
        for game in schedule.dates.pop().unwrap().games.into_iter() {
            games.push(Game {
                headline: game.content.editorial.recap.home.headline.clone(),
                subhead: game.content.editorial.recap.home.subhead.clone(),
                large: Photo::new(game.content.editorial.recap.home.photo.cuts.large.src),
                small: Photo::new(game.content.editorial.recap.home.photo.cuts.small.src),
            });
        }
        Schedule { games, cursor: 0 }
    }
}

pub enum Snippet<'a> {
    Small(&'a RgbaImage),
    Large(&'a RgbaImage, &'a str, &'a str),
}

pub struct Game {
    pub headline: String,
    pub subhead: String,
    large: Photo,
    small: Photo,
}

pub struct Photo {
    photo: Option<RgbaImage>,
    channel: crossbeam_channel::Receiver<RgbaImage>,
}

impl Photo {
    /// Constructs a new photo from the given source url.
    ///
    /// The function returns immediately, however the physical photo has been fired off
    /// as an ansynchronous download. Any attempts to the acquire with underlying RGBa will
    /// return None until the media is ready.
    ///
    /// If the download fails then this photo will return None indefinitely and an entry will
    /// be logged to stderr.
    pub fn new(src: String) -> Photo {
        let (tx, rx) = crossbeam_channel::bounded(1);
        tokio::task::spawn(async move {
            let url: hyper::Uri = match src.parse() {
                Ok(uri) => uri,
                Err(err) => {
                    eprintln!("Failed to parse {} as a URL", src);
                    eprintln!("Error: {}", err);
                    return;
                }
            };
            let https = hyper_tls::HttpsConnector::new();
            let resp = match hyper::Client::builder()
                .build::<_, hyper::Body>(https)
                .get(url)
                .await
            {
                Ok(resp) => resp,
                Err(err) => {
                    eprintln!("Failed to establish connection to {}", src);
                    eprintln!("Error: {}", err);
                    return;
                }
            };
            let buf = match hyper::body::to_bytes(resp).await {
                Ok(bytes) => bytes,
                Err(err) => {
                    eprintln!("Failed to download photo from {}", src);
                    eprintln!("Error: {}", err);
                    return;
                }
            };
            let img = match image::load_from_memory_with_format(&buf, ImageFormat::JPEG) {
                Ok(image) => image.into_rgba(),
                Err(err) => {
                    eprintln!("Image retrieved from {} failed to parse as a JPEG", src);
                    eprintln!("Error: {}", err);
                    return;
                }
            };
            match tx.send(img) {
                Ok(_) => (),
                Err(err) => {
                    eprintln!(
                        "Failed to send the downloaded contents of {} to the main thread",
                        src
                    );
                    eprintln!("Error: {}", err);
                    return;
                }
            }
        });
        Photo {
            photo: None,
            channel: rx,
        }
    }

    /// Retrieves the RGBa of this photo. Returns None if the photo has not
    /// completed its download.
    pub fn get(&mut self) -> Option<&RgbaImage> {
        if self.photo.is_some() {
            return self.photo.as_ref();
        }
        match self.channel.try_recv() {
            Ok(image) => {
                self.photo = Some(image);
                self.photo.as_ref()
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Testing that a broken send channel due to a failed download
    /// doesn't unexpectedly panic us or something.
    fn broken_photo_channel() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let mut photo = Photo {
            photo: None,
            channel: rx,
        };
        assert!(photo.get().is_none());
        drop(tx);
        assert!(photo.get().is_none());
    }
}
