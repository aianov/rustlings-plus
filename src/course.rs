use anyhow::{Context, Result, bail};
use serde::{Deserialize, Deserializer};
use std::{collections::BTreeMap, collections::HashMap, ops::Range};

use crate::{embedded::ContentFs, i18n::Locale};

pub const CURRENT_FORMAT_VERSION: u8 = 2;
pub const DEFAULT_TEST_COMMAND: [&str; 3] = ["cargo", "test", "--workspace"];

#[derive(Clone, Default, Debug, PartialEq)]
pub enum Text {
    #[default]
    Empty,
    Plain(String),
    Map(BTreeMap<String, String>),
}

impl Text {
    pub fn get(&self, locale: Locale) -> &str {
        match self {
            Text::Empty => "",
            Text::Plain(s) => s,
            Text::Map(map) => map
                .get(locale.as_str())
                .or_else(|| map.get("en"))
                .or_else(|| map.get("ru"))
                .or_else(|| map.values().next())
                .map_or("", String::as_str),
        }
    }
}

impl<'de> Deserialize<'de> for Text {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Plain(String),
            Map(BTreeMap<String, String>),
        }
        Ok(match Raw::deserialize(deserializer)? {
            Raw::Plain(s) => Text::Plain(s),
            Raw::Map(map) => Text::Map(map),
        })
    }
}

#[derive(Clone, Default, Debug, PartialEq)]
pub enum Steps {
    #[default]
    Empty,
    Plain(Vec<String>),
    Map(BTreeMap<String, Vec<String>>),
}

impl Steps {
    pub fn get(&self, locale: Locale) -> &[String] {
        match self {
            Steps::Empty => &[],
            Steps::Plain(steps) => steps,
            Steps::Map(map) => map
                .get(locale.as_str())
                .or_else(|| map.get("en"))
                .or_else(|| map.get("ru"))
                .or_else(|| map.values().next())
                .map_or(&[], Vec::as_slice),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    fn into_vec(self) -> Vec<String> {
        match self {
            OneOrMany::One(s) if s.trim().is_empty() => Vec::new(),
            OneOrMany::One(s) => vec![s],
            OneOrMany::Many(v) => v,
        }
    }
}

impl<'de> Deserialize<'de> for Steps {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Plain(OneOrMany),
            Map(BTreeMap<String, OneOrMany>),
        }
        Ok(match Raw::deserialize(deserializer)? {
            Raw::Plain(v) => Steps::Plain(v.into_vec()),
            Raw::Map(map) => Steps::Map(map.into_iter().map(|(k, v)| (k, v.into_vec())).collect()),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    File,
    Project,
    Milestone,
    Question,
}

#[derive(Debug, Default)]
pub struct Question {
    pub text: Text,
    pub options: Steps,
    pub answer: Vec<usize>,
    pub answer_text: Vec<String>,
    pub explain: Text,
}

impl Question {
    pub fn is_free_text(&self) -> bool {
        !self.answer_text.is_empty()
    }

    pub fn is_multi(&self) -> bool {
        self.answer.len() > 1
    }

    pub fn check_options(&self, chosen: &[usize]) -> bool {
        let mut chosen = chosen.to_vec();
        chosen.sort_unstable();
        chosen.dedup();
        let mut answer = self.answer.clone();
        answer.sort_unstable();
        answer.dedup();
        !answer.is_empty() && chosen == answer
    }

    pub fn check_text(&self, text: &str) -> bool {
        let given = text.trim().to_lowercase();
        self.answer_text
            .iter()
            .any(|a| a.trim().to_lowercase() == given)
    }
}

#[derive(Debug)]
pub struct Track {
    pub id: String,
    pub title: Text,
    pub description: Text,
    pub dependencies: toml::Table,
    pub chapters: Range<usize>,
}

#[derive(Debug)]
pub struct Visual {
    pub file: String,
    pub title: Text,
}

#[derive(Debug)]
pub struct Chapter {
    pub id: String,
    pub track: usize,
    pub title: Text,
    pub visuals: Vec<Visual>,
    pub exercises: Range<usize>,
    pub rel: String,
}

impl Chapter {
    pub fn exercises_dir(&self) -> String {
        format!("exercises/{}", self.rel)
    }

    pub fn readme(&self) -> String {
        format!("exercises/{}/README.md", self.rel)
    }
}

#[derive(Debug)]
pub struct Exercise {
    pub name: String,
    pub kind: Kind,
    pub track: usize,
    pub chapter: usize,
    pub journey: Option<usize>,
    pub test: bool,
    pub strict_clippy: bool,
    pub quiz: bool,
    pub requires: Vec<String>,
    pub input_files: Vec<String>,
    pub test_command: Vec<String>,
    pub locked: Vec<String>,
    pub hint: Steps,
    pub why: Text,
    pub exam: bool,
    pub style: Option<String>,
    pub question: Question,
    pub story: Text,
    pub src: String,
    pub sol: String,
}

impl Exercise {
    pub fn is_file(&self) -> bool {
        self.kind == Kind::File
    }

    pub fn hints(&self, locale: Locale) -> &[String] {
        if self.exam {
            &[]
        } else {
            self.hint.get(locale)
        }
    }
}

#[derive(Debug)]
pub struct Journey {
    pub id: String,
    pub dir: String,
    pub title: Text,
    pub milestones: Vec<String>,
}

pub struct Course {
    pub fs: &'static ContentFs,
    pub default_locale: Locale,
    pub welcome_message: Text,
    pub final_message: Text,
    pub tracks: Vec<Track>,
    pub chapters: Vec<Chapter>,
    pub exercises: Vec<Exercise>,
    pub journeys: Vec<Journey>,
    by_name: HashMap<String, usize>,
}

#[derive(Deserialize)]
struct RawCourse {
    format_version: u8,
    #[serde(default)]
    default_locale: Option<String>,
    #[serde(default)]
    welcome_message: Text,
    #[serde(default)]
    final_message: Text,
    #[serde(default)]
    tracks: Vec<RawTrack>,
    #[serde(default)]
    journeys: Vec<RawJourney>,
}

#[derive(Deserialize)]
struct RawTrack {
    id: String,
    #[serde(default)]
    title: Text,
    #[serde(default)]
    description: Text,
    #[serde(default)]
    chapters: Vec<String>,
    #[serde(default)]
    dependencies: toml::Table,
}

fn default_journey_dir() -> String {
    "backend".to_owned()
}

#[derive(Deserialize)]
struct RawJourney {
    id: String,
    #[serde(default = "default_journey_dir")]
    dir: String,
    #[serde(default)]
    title: Text,
    #[serde(default)]
    milestones: Vec<String>,
}

#[derive(Deserialize)]
struct RawChapterFile {
    #[serde(default)]
    chapter: RawChapter,
    #[serde(default)]
    exercises: Vec<RawExercise>,
}

#[derive(Deserialize, Default)]
struct RawChapter {
    #[serde(default)]
    title: Text,
    #[serde(default)]
    visuals: Vec<RawVisual>,
}

#[derive(Deserialize)]
struct RawVisual {
    file: String,
    #[serde(default)]
    title: Text,
}

const fn default_true() -> bool {
    true
}

fn command<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Vec<String>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Line(String),
        Argv(Vec<String>),
    }
    Ok(Some(match Raw::deserialize(deserializer)? {
        Raw::Line(line) => line.split_whitespace().map(str::to_owned).collect(),
        Raw::Argv(argv) => argv,
    }))
}

#[derive(Deserialize)]
struct RawExercise {
    name: String,
    #[serde(default)]
    kind: Kind,
    #[serde(default)]
    journey: Option<String>,
    #[serde(default = "default_true")]
    test: bool,
    #[serde(default)]
    strict_clippy: bool,
    #[serde(default)]
    quiz: bool,
    #[serde(default)]
    requires: Vec<String>,
    #[serde(default)]
    input_files: Vec<String>,
    #[serde(default, deserialize_with = "command")]
    test_command: Option<Vec<String>>,
    #[serde(default)]
    locked: Vec<String>,
    #[serde(default)]
    hint: Steps,
    #[serde(default)]
    why: Text,
    #[serde(default)]
    exam: bool,
    #[serde(default)]
    style: Option<String>,
    #[serde(default)]
    question: Text,
    #[serde(default)]
    options: Steps,
    #[serde(default)]
    answer: Vec<usize>,
    #[serde(default)]
    answer_text: Vec<String>,
    #[serde(default)]
    explain: Text,
}

#[derive(Deserialize, Default)]
pub struct RawMilestone {
    #[serde(default)]
    hint: Option<Steps>,
    #[serde(default)]
    requires: Option<Vec<String>>,
    #[serde(default, deserialize_with = "command")]
    test_command: Option<Vec<String>>,
    #[serde(default)]
    story: Option<Text>,
}

fn utf8(fs: &ContentFs, path: &str) -> Result<Option<&'static str>> {
    fs.get(path)
        .map(|b| str::from_utf8(b).with_context(|| format!("{path} is not valid UTF-8")))
        .transpose()
}

impl Course {
    pub fn load(fs: &'static ContentFs) -> Result<Self> {
        let course_toml =
            utf8(fs, "course/course.toml")?.context("The course has no `course/course.toml`")?;
        let raw: RawCourse =
            toml::de::from_str(course_toml).context("Failed to parse `course/course.toml`")?;
        if raw.format_version > CURRENT_FORMAT_VERSION {
            bail!("{}", crate::i18n::tr().format_too_new);
        }

        let journeys = raw
            .journeys
            .into_iter()
            .map(|j| Journey {
                id: j.id,
                dir: j.dir.trim_end_matches('/').to_owned(),
                title: j.title,
                milestones: j.milestones,
            })
            .collect::<Vec<_>>();

        let mut tracks = Vec::with_capacity(raw.tracks.len());
        let mut chapters = Vec::new();
        let mut exercises = Vec::new();
        let mut by_name = HashMap::new();

        for (track_ind, raw_track) in raw.tracks.into_iter().enumerate() {
            let chapters_start = chapters.len();
            for chapter_id in &raw_track.chapters {
                let rel = format!("{}/{chapter_id}", raw_track.id);
                let chapter_toml_path = format!("exercises/{rel}/chapter.toml");
                let Some(chapter_toml) = utf8(fs, &chapter_toml_path)? else {
                    continue;
                };
                let raw_chapter: RawChapterFile = toml::de::from_str(chapter_toml)
                    .with_context(|| format!("Failed to parse `{chapter_toml_path}`"))?;

                let chapter_ind = chapters.len();
                let exercises_start = exercises.len();
                for raw_ex in raw_chapter.exercises {
                    if by_name
                        .insert(raw_ex.name.clone(), exercises.len())
                        .is_some()
                    {
                        bail!(
                            "The exercise name `{}` is used twice (`{chapter_toml_path}`)",
                            raw_ex.name
                        );
                    }
                    let mut ex = Exercise {
                        src: match raw_ex.kind {
                            Kind::File => format!("exercises/{rel}/{}.rs", raw_ex.name),
                            Kind::Project | Kind::Milestone => {
                                format!("exercises/{rel}/{}", raw_ex.name)
                            }
                            Kind::Question => format!("exercises/{rel}/README.md"),
                        },
                        sol: match raw_ex.kind {
                            Kind::File => format!("solutions/{rel}/{}.rs", raw_ex.name),
                            Kind::Project | Kind::Milestone => {
                                format!("solutions/{rel}/{}", raw_ex.name)
                            }
                            Kind::Question => String::new(),
                        },
                        journey: None,
                        name: raw_ex.name,
                        kind: raw_ex.kind,
                        track: track_ind,
                        chapter: chapter_ind,
                        test: raw_ex.test,
                        strict_clippy: raw_ex.strict_clippy,
                        quiz: raw_ex.quiz,
                        requires: raw_ex.requires,
                        input_files: raw_ex.input_files,
                        test_command: raw_ex.test_command.unwrap_or_else(|| {
                            DEFAULT_TEST_COMMAND
                                .iter()
                                .map(|s| (*s).to_owned())
                                .collect()
                        }),
                        locked: raw_ex.locked,
                        hint: raw_ex.hint,
                        why: raw_ex.why,
                        exam: raw_ex.exam,
                        style: raw_ex.style,
                        question: Question {
                            text: raw_ex.question,
                            options: raw_ex.options,
                            answer: raw_ex.answer,
                            answer_text: raw_ex.answer_text,
                            explain: raw_ex.explain,
                        },
                        story: Text::Empty,
                    };
                    if ex.kind == Kind::Milestone {
                        let journey_id = raw_ex.journey.as_deref().unwrap_or("backend");
                        ex.journey = journeys.iter().position(|j| j.id == journey_id);
                        if ex.journey.is_none() {
                            bail!(
                                "The milestone `{}` refers to an unknown journey `{journey_id}`",
                                ex.name
                            );
                        }
                        let milestone_toml_path = format!("{}/milestone.toml", ex.src);
                        if let Some(milestone_toml) = utf8(fs, &milestone_toml_path)? {
                            let meta: RawMilestone = toml::de::from_str(milestone_toml)
                                .with_context(|| {
                                    format!("Failed to parse `{milestone_toml_path}`")
                                })?;
                            if let Some(hint) = meta.hint {
                                ex.hint = hint;
                            }
                            if let Some(requires) = meta.requires {
                                ex.requires = requires;
                            }
                            if let Some(test_command) = meta.test_command {
                                ex.test_command = test_command;
                            }
                            if let Some(story) = meta.story {
                                ex.story = story;
                            }
                        }
                    }
                    if ex.test_command.is_empty() {
                        bail!("The exercise `{}` has an empty `test_command`", ex.name);
                    }
                    exercises.push(ex);
                }

                chapters.push(Chapter {
                    id: chapter_id.clone(),
                    track: track_ind,
                    title: raw_chapter.chapter.title,
                    visuals: raw_chapter
                        .chapter
                        .visuals
                        .into_iter()
                        .map(|v| Visual {
                            file: v.file,
                            title: v.title,
                        })
                        .collect(),
                    exercises: exercises_start..exercises.len(),
                    rel,
                });
            }

            tracks.push(Track {
                id: raw_track.id,
                title: raw_track.title,
                description: raw_track.description,
                dependencies: raw_track.dependencies,
                chapters: chapters_start..chapters.len(),
            });
        }

        if exercises.is_empty() {
            bail!("The course has no exercises yet");
        }

        let default_locale = raw
            .default_locale
            .as_deref()
            .and_then(Locale::parse)
            .unwrap_or(Locale::En);

        Ok(Self {
            fs,
            default_locale,
            welcome_message: raw.welcome_message,
            final_message: raw.final_message,
            tracks,
            chapters,
            exercises,
            journeys,
            by_name,
        })
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }

    pub fn find_track(&self, id: &str) -> Option<usize> {
        self.tracks.iter().position(|t| t.id == id)
    }

    pub fn chapter_of(&self, ex: usize) -> &Chapter {
        &self.chapters[self.exercises[ex].chapter]
    }

    pub fn track_exercises(&self, track: usize) -> Range<usize> {
        let chapters = &self.tracks[track].chapters;
        if chapters.is_empty() {
            return 0..0;
        }
        self.chapters[chapters.start].exercises.start..self.chapters[chapters.end - 1].exercises.end
    }

    pub fn journey_milestones(&self, journey: usize) -> Vec<Option<usize>> {
        self.journeys[journey]
            .milestones
            .iter()
            .map(|name| {
                self.find(name)
                    .filter(|&ind| self.exercises[ind].kind == Kind::Milestone)
            })
            .collect()
    }

    pub fn milestone_position(&self, ex: usize) -> Option<(usize, usize)> {
        let journey = self.exercises[ex].journey?;
        let pos = self.journeys[journey]
            .milestones
            .iter()
            .position(|m| *m == self.exercises[ex].name)?;
        Some((journey, pos))
    }

    pub fn track_has_file_exercises(&self, track: usize) -> bool {
        self.exercises[self.track_exercises(track)]
            .iter()
            .any(Exercise::is_file)
    }
}

pub fn package_name(track_id: &str) -> String {
    format!("{track_id}-exercises")
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn fixture() -> Vec<(&'static str, &'static [u8])> {
        vec![
            (
                "course/course.toml",
                br#"format_version = 2
default_locale = "ru"
welcome_message.en = "hi"
welcome_message.ru = "privet"

[[tracks]]
id = "basics"
title.en = "Basics"
title.ru = "Osnovy"
chapters = ["00_intro", "01_missing", "02_more"]

[tracks.dependencies]

[[tracks]]
id = "svc"
title = "Services"
chapters = ["00_arch"]

[tracks.dependencies]
course-support = { path = "@support", features = ["pg"] }
tokio = { version = "1", features = ["full"] }

[[journeys]]
id = "backend"
title.ru = "Soc"
milestones = ["ms_one", "ms_two"]
"#,
            ),
            (
                "exercises/basics/00_intro/chapter.toml",
                br#"[chapter]
title = "Intro"

[[chapter.visuals]]
file = "visuals/a.html"
title.ru = "A"

[[exercises]]
name = "intro1"
hint.en = "en hint"
hint.ru = "ru hint"

[[exercises]]
name = "intro2"
test = false
quiz = true
"#,
            ),
            ("exercises/basics/00_intro/README.md", b"readme en"),
            ("exercises/basics/00_intro/README.ru.md", b"readme ru"),
            ("exercises/basics/00_intro/intro1.rs", b"// en\nfn main() {}\n"),
            ("exercises/basics/00_intro/intro1.ru.rs", b"// ru\nfn main() {}\n"),
            ("exercises/basics/00_intro/intro2.rs", b"fn main() {}\n"),
            ("exercises/basics/00_intro/visuals/a.html", b"<html></html>"),
            (
                "exercises/basics/02_more/chapter.toml",
                br#"[chapter]
title = "More"

[[exercises]]
name = "proj"
kind = "project"
locked = ["tests/t.rs"]

[[exercises]]
name = "ms_one"
kind = "milestone"
journey = "backend"
quiz = true
"#,
            ),
            ("exercises/basics/02_more/README.md", b"more"),
            ("exercises/basics/02_more/proj/Cargo.toml", b"[workspace]\n"),
            ("exercises/basics/02_more/proj/tests/t.rs", b"// test\n"),
            ("exercises/basics/02_more/ms_one/README.md", b"ms readme"),
            ("exercises/basics/02_more/ms_one/arch.toml", b""),
            (
                "exercises/basics/02_more/ms_one/milestone.toml",
                b"hint = \"ms hint\"\nrequires = [\"postgres\"]\ntest_command = \"cargo test -p x\"\nstory.ru = \"story\"\n",
            ),
            ("exercises/basics/02_more/ms_one/inject/tests/a.rs", b"inject"),
            ("exercises/basics/02_more/ms_one/scaffold/src/lib.rs", b"scaffold"),
            ("solutions/basics/00_intro/intro1.rs", b"// sol en\nfn main() {}\n"),
            ("solutions/basics/00_intro/intro1.ru.rs", b"// sol ru\nfn main() {}\n"),
            ("solutions/basics/02_more/ms_one/Cargo.toml", b"[workspace]\n"),
            ("solutions/basics/02_more/ms_one/src/lib.rs", b"lib"),
            (
                "exercises/svc/00_arch/chapter.toml",
                br#"[chapter]
title.ru = "Arch"

[[exercises]]
name = "arch1"
requires = ["postgres"]
"#,
            ),
            ("exercises/svc/00_arch/arch1.rs", b"fn main() {}\n"),
        ]
    }

    pub fn fixture_fs() -> &'static ContentFs {
        Box::leak(Box::new(ContentFs::from_entries(&fixture())))
    }

    pub fn fixture_course() -> &'static Course {
        Box::leak(Box::new(Course::load(fixture_fs()).unwrap()))
    }

    #[test]
    fn text_fallback() {
        let mut map = BTreeMap::new();
        map.insert("ru".to_owned(), "ru".to_owned());
        let text = Text::Map(map.clone());
        assert_eq!(text.get(Locale::En), "ru");
        map.insert("en".to_owned(), "en".to_owned());
        let text = Text::Map(map);
        assert_eq!(text.get(Locale::Ru), "ru");
        assert_eq!(text.get(Locale::En), "en");
        assert_eq!(Text::Plain("p".to_owned()).get(Locale::Ru), "p");
        assert_eq!(Text::Empty.get(Locale::Ru), "");
    }

    #[test]
    fn parse_fixture() {
        let course = Course::load(fixture_fs()).unwrap();
        assert_eq!(course.default_locale, Locale::Ru);
        assert_eq!(course.welcome_message.get(Locale::Ru), "privet");
        assert_eq!(course.tracks.len(), 2);
        assert_eq!(course.tracks[0].chapters, 0..2);
        assert_eq!(course.chapters.len(), 3);
        assert_eq!(course.exercises.len(), 5);
        let names = course
            .exercises
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["intro1", "intro2", "proj", "ms_one", "arch1"]);

        let intro1 = &course.exercises[0];
        assert_eq!(intro1.src, "exercises/basics/00_intro/intro1.rs");
        assert_eq!(intro1.sol, "solutions/basics/00_intro/intro1.rs");
        assert!(intro1.test);
        assert_eq!(intro1.hint.get(Locale::Ru), ["ru hint"]);
        let intro2 = &course.exercises[1];
        assert!(!intro2.test && intro2.quiz);
        assert_eq!(course.chapters[0].visuals[0].file, "visuals/a.html");
        assert_eq!(course.chapters[0].visuals[0].title.get(Locale::En), "A");

        let proj = &course.exercises[2];
        assert_eq!(proj.kind, Kind::Project);
        assert_eq!(proj.src, "exercises/basics/02_more/proj");
        assert_eq!(proj.test_command, ["cargo", "test", "--workspace"]);
        assert_eq!(proj.locked, ["tests/t.rs"]);

        let ms = &course.exercises[3];
        assert_eq!(ms.kind, Kind::Milestone);
        assert_eq!(ms.journey, Some(0));
        assert_eq!(ms.hint.get(Locale::En), ["ms hint"]);
        assert_eq!(ms.requires, ["postgres"]);
        assert_eq!(ms.story.get(Locale::Ru), "story");
        assert_eq!(ms.test_command, ["cargo", "test", "-p", "x"]);
        assert_eq!(course.milestone_position(3), Some((0, 0)));
        assert_eq!(course.journey_milestones(0), vec![Some(3), None]);
        assert_eq!(course.journeys[0].dir, "backend");

        assert_eq!(course.track_exercises(0), 0..4);
        assert_eq!(course.track_exercises(1), 4..5);
        assert_eq!(course.find("arch1"), Some(4));
        assert!(course.track_has_file_exercises(1));
        assert_eq!(course.chapters[2].title.get(Locale::En), "Arch");
    }

    #[test]
    fn steps_and_questions() {
        let toml = r#"
[chapter]
title = "q"

[[exercises]]
name = "q1"
kind = "question"
question.ru = "Вопрос?"
options.ru = ["a", "b", "c"]
options.en = ["A", "B", "C"]
answer = [2, 0]
explain = "because"
hint.en = "one"
hint.ru = ["раз", "два"]
why.ru = "так проще"

[[exercises]]
name = "q2"
kind = "question"
question = "?"
answer_text = [" Forty Two "]

[[exercises]]
name = "e1"
kind = "project"
exam = true
style = "scratch"
hint = ["x", "y"]
"#;
        let mut files = fixture();
        files.push(("exercises/basics/01_missing/chapter.toml", toml.as_bytes()));
        let course = Course::load(Box::leak(Box::new(ContentFs::from_entries(&files)))).unwrap();
        let q1 = &course.exercises[course.find("q1").unwrap()];
        assert_eq!(q1.kind, Kind::Question);
        assert_eq!(q1.hint.get(Locale::Ru), ["раз", "два"]);
        assert_eq!(q1.hint.get(Locale::En), ["one"]);
        assert_eq!(q1.why.get(Locale::En), "так проще");
        assert_eq!(q1.question.options.get(Locale::En), ["A", "B", "C"]);
        assert!(q1.question.is_multi() && !q1.question.is_free_text());
        assert!(q1.question.check_options(&[0, 2]));
        assert!(q1.question.check_options(&[2, 0, 2]));
        assert!(!q1.question.check_options(&[0]));
        assert!(!q1.question.check_options(&[0, 1, 2]));
        assert_eq!(q1.src, "exercises/basics/01_missing/README.md");
        let q2 = &course.exercises[course.find("q2").unwrap()];
        assert!(q2.question.is_free_text());
        assert!(q2.question.check_text("forty two"));
        assert!(q2.question.check_text("  FORTY TWO"));
        assert!(!q2.question.check_text("42"));
        assert!(!q2.question.check_options(&[]));
        let e1 = &course.exercises[course.find("e1").unwrap()];
        assert!(e1.exam);
        assert_eq!(e1.style.as_deref(), Some("scratch"));
        assert_eq!(e1.hint.get(Locale::Ru), ["x", "y"]);
        assert!(e1.hints(Locale::Ru).is_empty());
    }

    #[test]
    fn duplicate_names_fail() {
        let mut files = fixture();
        files.push((
            "exercises/basics/01_missing/chapter.toml",
            b"[chapter]\ntitle = \"x\"\n[[exercises]]\nname = \"intro1\"\n",
        ));
        assert!(Course::load(Box::leak(Box::new(ContentFs::from_entries(&files)))).is_err());
    }

    #[test]
    fn real_course_parses() {
        let fs = crate::embedded::init().unwrap();
        let course = Course::load(fs).unwrap();
        assert!(course.find("intro1").is_some());
        for ex in &course.exercises {
            match ex.kind {
                Kind::File | Kind::Question => assert!(fs.exists(&ex.src), "{}", ex.src),
                Kind::Project | Kind::Milestone => assert!(fs.has_dir(&ex.src), "{}", ex.src),
            }
        }
    }
}
