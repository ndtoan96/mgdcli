use super::MangadexError;
use getset::Getters;
use reqwest::IntoUrl;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MangaQuery {
    #[serde(skip)]
    pub(crate) id: String,
    pub(crate) groups: Vec<String>,
    pub(crate) translated_language: Vec<String>,
}

#[derive(Debug, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct Volume {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    volume: Option<f32>,
    count: usize,
    chapters: HashMap<String, Chapter>,
}

#[derive(Debug, Deserialize, Getters, PartialEq, PartialOrd)]
#[getset(get = "pub")]
pub struct Chapter {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    chapter: Option<f32>,
    id: String,
    count: usize,
    others: Vec<String>,
}

fn deserialize_number_from_string<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: String = Deserialize::deserialize(deserializer)?;
    let num = raw.parse::<f32>().ok();
    Ok(num)
}

impl MangaQuery {
    pub fn new(id: impl ToString) -> Self {
        Self {
            id: id.to_string(),
            groups: Vec::new(),
            translated_language: Vec::new(),
        }
    }

    pub fn from_url(url: impl IntoUrl + Clone + ToString) -> Result<Self, MangadexError> {
        let url = url
            .clone()
            .into_url()
            .map_err(|_e| MangadexError::UrlParseError(url.to_string()))?;
        if !url.domain().is_some_and(|x| x == "mangadex.org") {
            return Err(MangadexError::UrlParseError(url.to_string()));
        }
        if let Some(mut segments) = url.path_segments() {
            if segments.next().is_some_and(|x| x == "title") {
                if let Some(id) = segments.next() {
                    Ok(Self::new(id))
                } else {
                    Err(MangadexError::UrlParseError(url.to_string()))
                }
            } else {
                Err(MangadexError::UrlParseError(url.to_string()))
            }
        } else {
            Err(MangadexError::UrlParseError(url.to_string()))
        }
    }

    pub fn group(mut self, group: impl ToString) -> Self {
        self.groups.push(group.to_string());
        self
    }

    pub fn language(mut self, language: impl ToString) -> Self {
        self.translated_language.push(language.to_string());
        self
    }

    pub async fn execute(self) -> Result<Vec<Volume>, MangadexError> {
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        pub(crate) enum ResponseBody {
            #[allow(dead_code)]
            Empty {
                volumes: [EmptyType; 0],
            },
            NonEmpty {
                volumes: HashMap<String, Volume>,
            },
        }

        #[derive(Debug, Deserialize)]
        pub(crate) struct EmptyType;

        let mut query = Vec::new();
        for group in &self.groups {
            query.push(("group[]", group));
        }

        for language in &self.translated_language {
            query.push(("translatedLanguage[]", language));
        }

        let bytes = reqwest::Client::new()
            .get(format!(
                "https://api.mangadex.org/manga/{}/aggregate",
                self.id
            ))
            .query(&query)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        let response: ResponseBody = serde_json::from_slice(&bytes)?;
        match response {
            ResponseBody::Empty { .. } => Ok(Vec::new()),
            ResponseBody::NonEmpty { volumes } => Ok(volumes.into_values().collect()),
        }
    }
}

pub trait GetChapters<'a> {
    fn get_chapters(&self) -> Vec<&'a Chapter>;
}

impl<'a, T> GetChapters<'a> for T
where
    T: IntoIterator<Item = &'a Volume> + Clone,
{
    fn get_chapters(&self) -> Vec<&'a Chapter> {
        let mut chapters: Vec<&Chapter> = self
            .clone()
            .into_iter()
            .flat_map(|v| v.chapters().values())
            .collect();
        chapters.sort_by(|x, y| match (x.chapter, y.chapter) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(c1), Some(c2)) => c1.total_cmp(&c2),
        });
        chapters
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_manga_query() {
        let volumes = MangaQuery::new("d7037b2a-874a-4360-8a7b-07f2899152fd")
            .language("fr")
            .execute()
            .await
            .unwrap();
        assert!(!volumes.is_empty());

        let volumes = MangaQuery::new("d7037b2a-874a-4360-8a7b-07f2899152fd")
            .language("xxx")
            .execute()
            .await
            .unwrap();
        assert!(volumes.is_empty());
    }

    #[test]
    fn test_url_parse() {
        assert!(MangaQuery::from_url("https://mangadex.org/title/99b8eaeb-9041-4bfd-8eb7-d72addc88eb7/the-cafe-terrace-and-its-goddesses").is_ok());
        assert!(MangaQuery::from_url("https://mangadex.org/chapter/99b8eaeb-9041-4bfd-8eb7-d72addc88eb7/the-cafe-terrace-and-its-goddesses").is_err());
        assert!(MangaQuery::from_url("https://mangapark.com/title/99b8eaeb-9041-4bfd-8eb7-d72addc88eb7/the-cafe-terrace-and-its-goddesses").is_err());
    }
}
