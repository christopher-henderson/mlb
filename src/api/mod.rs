use serde::Deserialize;
use std::fmt::Formatter;

#[derive(Deserialize)]
pub struct Schedule {
    pub copyright: String,
    pub dates: Vec<Date>,
}

pub static DEFAULT: &str = "http://statsapi.mlb.com/api/v1/schedule?hydrate=\
    game(content(editorial(recap))),decisions&date=2018-06-10&sportId=1";

impl Schedule {
    /// I do not believe that there is an async version of std::convert provided by anyone.
    /// This'd be a good point of conversation if you know otherwise because, of course,
    /// the ecosystem is so fragmented at the moment (although rapidly rallying...).
    ///
    /// When taking in an argument that is really solely meant to be passed on down to a
    /// dependency it is generally considered polite to be at least as, if not LESS, specific.
    /// Since we know that there is an std::str:parse implementation for getting the URI that
    /// we want then we can constraint on any type that can represent itself as a str. This
    /// lets the caller be more carefree on their types and lets the templating
    /// system take the wheel. E.G.
    ///
    ///  ```
    /// let url = "https://black.coffee".to_string();
    /// Schedule::try_from(&url).unwrap(); // as a borrow
    /// Schedule::try_from(url).unwrap(); // as a move
    /// Schedule::try_from("https://backflip.gov").unwrap(); // as a str literal, etc.
    /// ```
    ///
    /// Of course, the tradeoffs of monomorphization versus taking in a dynamic trait is whether
    /// your resource constraint is the binary footprint or the runtime CPU. That is, this function
    /// will get code generated for every different way that it is called in the target binary
    /// which increases the raw size of the binary. Alternatively, a Box::<dyn trait> incurs
    /// the wrath of a fat pointer with a dynamic lookup to the concrete type. Pick your poison.
    pub async fn try_from<T: AsRef<str>>(src: T) -> APIResult<Schedule> {
        let target = src.as_ref().parse::<hyper::Uri>().map_err(|err| APIError {
            src: src.as_ref().to_string(),
            context: ErrorContext::URIParsing,
            original: err.to_string(),
        })?;
        let resp = hyper::Client::default()
            .get(target)
            .await
            .map_err(|err| APIError {
                src: src.as_ref().to_string(),
                context: ErrorContext::ConnectionEstablishment,
                original: err.to_string(),
            })?;
        let buf = hyper::body::to_bytes(resp).await.map_err(|err| APIError {
            src: src.as_ref().to_string(),
            context: ErrorContext::Downloading,
            original: err.to_string(),
        })?;
        serde_json::from_slice(&buf).map_err(|err| APIError {
            src: src.as_ref().to_string(),
            context: ErrorContext::Deserializing,
            original: err.to_string(),
        })
    }
}

#[derive(Deserialize)]
pub struct Date {
    pub date: String,
    pub games: Vec<Game>,
}

#[derive(Deserialize)]
pub struct Game {
    pub content: Content,
}

#[derive(Deserialize)]
pub struct Content {
    pub editorial: Editorial,
}

#[derive(Deserialize)]
pub struct Editorial {
    pub recap: Recap,
}

#[derive(Deserialize)]
pub struct Recap {
    pub home: Home,
}

#[derive(Deserialize)]
pub struct Home {
    pub headline: String,
    pub subhead: String,
    pub photo: Photos,
}

#[derive(Deserialize)]
pub struct Photos {
    pub cuts: Cuts,
}

#[derive(Deserialize)]
pub struct Cuts {
    #[serde(alias = "480x270")]
    pub large: Photo,
    #[serde(alias = "320x180")]
    pub small: Photo,
}

#[derive(Deserialize)]
pub struct Photo {
    pub width: u32,
    pub height: u32,
    pub src: String,
}

// I Decided to use this corner to show how one might create their own error types.
// I've also used boilerplate reducers in the past, such as error_chain, which help. But those
// are also more appropriate for top level application code that is trying to tie
// a bunch of APIs together rather than a library itself.
type APIResult<T> = Result<T, APIError>;

pub struct APIError {
    src: String,
    context: ErrorContext,
    original: String,
}

impl std::error::Error for APIError {}

impl std::fmt::Display for APIError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.write_fmt(format_args!(
            "{}. Error: {}. Source: {}",
            self.context, self.original, self.src
        ))
    }
}

impl std::fmt::Debug for APIError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.write_fmt(format_args!("{}", self))
    }
}

pub enum ErrorContext {
    URIParsing,
    ConnectionEstablishment,
    Downloading,
    Deserializing,
}

impl std::fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::URIParsing => f.write_str("Failed to parse the given API endpoint"),
            Self::ConnectionEstablishment => {
                f.write_str("Failed to establish a connection with the given API endpoint")
            }
            Self::Downloading => f.write_str("Failed to download data from the given API endpoint"),
            Self::Deserializing => {
                f.write_str("Failed to deserialize data from the given API endpoint")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_DATA: &[u8] = include_bytes!("test.json");

    #[test]
    fn smoke() {
        let _: Schedule = serde_json::from_slice(TEST_DATA).unwrap();
    }

    #[test]
    fn smoke_async_real_download() {
        // This just smoke checks that our api call is working.
        let _: Schedule = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(Schedule::try_from(DEFAULT))
            .unwrap();
    }
}
