use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use validation::{CharLen, Email, FieldError, Matches, MaxItems, Range, Rule, Username, Validator};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Patch<T> {
    Missing,
    Null,
    Value(T),
}

#[allow(clippy::derivable_impls)]
impl<T> Default for Patch<T> {
    fn default() -> Self {
        Patch::Missing
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Patch<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match Option::<T>::deserialize(deserializer)? {
            None => Patch::Null,
            Some(value) => Patch::Value(value),
        })
    }
}

fn problem(path: &str, code: &'static str, message: &str) -> Vec<FieldError> {
    vec![FieldError { path: path.to_string(), code, message: message.to_string() }]
}

fn parse_body<T: DeserializeOwned>(json: &str, allowed: &[&str]) -> Result<T, Vec<FieldError>> {
    let value: Value = serde_json::from_str(json).map_err(|e| problem("", "bad_json", &e.to_string()))?;
    let Some(object) = value.as_object() else {
        return Err(problem("", "bad_json", "ожидался объект"));
    };
    let unknown: Vec<FieldError> = object
        .keys()
        .filter(|key| !allowed.contains(&key.as_str()))
        .map(|key| FieldError { path: key.clone(), code: "unknown_field", message: "такого поля нет".to_string() })
        .collect();
    if !unknown.is_empty() {
        return Err(unknown);
    }
    serde_json::from_value(value).map_err(|e| problem("", "bad_json", &e.to_string()))
}

fn normalize_name(raw: &str) -> String {
    let visible: String = raw
        .chars()
        .filter(|c| !matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}'))
        .collect();
    visible.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_tag(raw: &str) -> String {
    raw.trim().trim_start_matches('#').split_whitespace().collect::<Vec<_>>().join("-").to_lowercase()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterBody {
    username: String,
    email: String,
    display_name: Option<String>,
    birth_year: Option<u16>,
    #[serde(default)]
    marketing_opt_in: bool,
}

#[derive(Debug, PartialEq)]
pub struct Registration {
    pub username: Username,
    pub email: Email,
    pub display_name: Option<String>,
    pub birth_year: Option<u16>,
    pub marketing_opt_in: bool,
}

pub fn parse_register(json: &str) -> Result<Registration, Vec<FieldError>> {
    let body: RegisterBody =
        parse_body(json, &["username", "email", "displayName", "birthYear", "marketingOptIn"])?;
    let mut v = Validator::new();
    let username = Username::parse(&body.username);
    if let Err(e) = &username {
        v.error("username", e.code, e.message.clone());
    }
    let email = Email::parse(&body.email);
    if let Err(e) = &email {
        v.error("email", e.code, e.message.clone());
    }
    let display_name = body.display_name.as_deref().map(normalize_name);
    if let Some(name) = &display_name {
        v.check("displayName", name.as_str(), &[&CharLen { min: 1, max: 30 }]);
    }
    if let Some(year) = body.birth_year {
        v.check("birthYear", &year, &[&Range { min: 1900u16, max: 2020 }]);
    }
    v.finish()?;
    Ok(Registration {
        username: username.expect("проверено выше"),
        email: email.expect("проверено выше"),
        display_name,
        birth_year: body.birth_year,
        marketing_opt_in: body.marketing_opt_in,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub display_name: String,
    pub bio: Option<String>,
    pub birth_year: Option<u16>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchBody {
    #[serde(default)]
    display_name: Patch<String>,
    #[serde(default)]
    bio: Patch<String>,
    #[serde(default)]
    birth_year: Patch<u16>,
}

pub fn apply_profile_patch(profile: &mut Profile, json: &str) -> Result<(), Vec<FieldError>> {
    let body: PatchBody = parse_body(json, &["displayName", "bio", "birthYear"])?;
    let mut v = Validator::new();
    let mut next = profile.clone();
    match body.display_name {
        Patch::Missing => {}
        Patch::Null => v.error("displayName", "null_not_allowed", "имя нельзя очистить"),
        Patch::Value(name) => {
            let name = normalize_name(&name);
            v.check("displayName", name.as_str(), &[&CharLen { min: 1, max: 30 }]);
            next.display_name = name;
        }
    }
    match body.bio {
        Patch::Missing => {}
        Patch::Null => next.bio = None,
        Patch::Value(bio) => {
            v.check("bio", bio.as_str(), &[&CharLen { min: 1, max: 160 }]);
            next.bio = Some(bio.trim().to_string());
        }
    }
    match body.birth_year {
        Patch::Missing => {}
        Patch::Null => next.birth_year = None,
        Patch::Value(year) => {
            v.check("birthYear", &year, &[&Range { min: 1900u16, max: 2020 }]);
            next.birth_year = Some(year);
        }
    }
    v.finish()?;
    *profile = next;
    Ok(())
}

#[derive(Deserialize)]
struct LinkBody {
    url: String,
    title: Option<String>,
}

#[derive(Deserialize)]
struct PostBody {
    text: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    links: Vec<LinkBody>,
}

#[derive(Debug, PartialEq)]
pub struct Link {
    pub url: String,
    pub title: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct NewPost {
    pub text: String,
    pub tags: Vec<String>,
    pub links: Vec<Link>,
}

pub fn parse_post(json: &str) -> Result<NewPost, Vec<FieldError>> {
    let body: PostBody = parse_body(json, &["text", "tags", "links"])?;
    let mut v = Validator::new();
    v.check("text", body.text.as_str(), &[&CharLen { min: 1, max: 280 }]);
    v.check("tags", &body.tags[..], &[&MaxItems(10)]);
    let tags: Vec<String> = body.tags.iter().map(|t| normalize_tag(t)).collect();
    v.scope("tags", |v| {
        for (index, tag) in tags.iter().enumerate() {
            let field = format!("[{index}]");
            if let Some((code, message)) = (CharLen { min: 1, max: 30 }).check(tag.as_str()) {
                v.error(&field, code, message);
            } else if tags[..index].contains(tag) {
                v.error(&field, "duplicate", "такой тег уже есть");
            }
        }
    });
    v.check("links", &body.links[..], &[&MaxItems(5)]);
    let url_format = Matches::new("https?://[^ ]+").expect("шаблон ссылки верен");
    v.each("links", &body.links, |v, link| {
        v.check("url", link.url.trim(), &[&CharLen { min: 1, max: 200 }, &url_format]);
        if let Some(title) = &link.title {
            v.check("title", title.as_str(), &[&CharLen { min: 0, max: 50 }]);
        }
    });
    v.finish()?;
    Ok(NewPost {
        text: body.text.trim().to_string(),
        tags,
        links: body.links.into_iter().map(|l| Link { url: l.url.trim().to_string(), title: l.title }).collect(),
    })
}
