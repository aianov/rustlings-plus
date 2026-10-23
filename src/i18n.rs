use std::sync::atomic::{AtomicU8, Ordering::Relaxed};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Locale {
    En,
    Ru,
}

impl Locale {
    pub fn as_str(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Ru => "ru",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "en" => Some(Locale::En),
            "ru" => Some(Locale::Ru),
            _ => None,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Locale::En => Locale::Ru,
            Locale::Ru => Locale::En,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Locale::En => "English",
            Locale::Ru => "Русский",
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_locale(locale: Locale) {
    CURRENT.store(locale as u8, Relaxed);
}

pub fn locale() -> Locale {
    if CURRENT.load(Relaxed) == Locale::Ru as u8 {
        Locale::Ru
    } else {
        Locale::En
    }
}

pub fn tr() -> &'static Strings {
    match locale() {
        Locale::En => &EN,
        Locale::Ru => &RU,
    }
}

pub fn fill(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::with_capacity(template.len() + 32);
    let mut rest = template;
    let mut args = args.iter();
    while let Some(ind) = rest.find("{}") {
        out.push_str(&rest[..ind]);
        if let Some(arg) = args.next() {
            out.push_str(&arg.to_string());
        }
        rest = &rest[ind + 2..];
    }
    out.push_str(rest);
    out
}

pub fn split2(template: &str) -> (&str, &str) {
    template.split_once("{}").unwrap_or((template, ""))
}

pub struct Strings {
    pub pre_init: &'static str,
    pub press_enter: &'static str,
    pub need_tty: &'static str,
    pub format_too_new: &'static str,
    pub old_layout: &'static str,
    pub migrate_prompt: &'static str,
    pub no_exercise: &'static str,
    pub unknown_track: &'static str,
    pub unknown_locale: &'static str,
    pub not_milestone: &'static str,

    pub ran_with_errors: &'static str,
    pub ran_ok: &'static str,
    pub skipped: &'static str,
    pub solution_for_comparison: &'static str,
    pub next_exercise: &'static str,
    pub one_pending: &'static str,
    pub many_pending: &'static str,
    pub skipped_in_check_all: &'static str,
    pub reset_done: &'static str,
    pub current_exercise: &'static str,
    pub hint: &'static str,
    pub no_hint: &'static str,
    pub checking_all: &'static str,
    pub check_done: &'static str,
    pub check_pending: &'static str,
    pub check_skip: &'static str,
    pub progress: &'static str,

    pub cargo_missing: &'static str,
    pub run_failed: &'static str,
    pub timed_out: &'static str,
    pub nonzero_exit: &'static str,
    pub output: &'static str,
    pub service_down: &'static str,
    pub checking: &'static str,
    pub project_missing: &'static str,

    pub exercise_done: &'static str,
    pub when_done: &'static str,
    pub track: &'static str,
    pub chapter: &'static str,
    pub exercise: &'static str,
    pub file: &'static str,
    pub folder: &'static str,
    pub task: &'static str,
    pub theory: &'static str,
    pub key_next: &'static str,
    pub key_run: &'static str,
    pub key_hint: &'static str,
    pub key_list: &'static str,
    pub key_reset: &'static str,
    pub key_quit: &'static str,
    pub key_lang: &'static str,
    pub key_visuals: &'static str,
    pub key_baseline: &'static str,
    pub quit_msg: &'static str,
    pub notify_err: &'static str,
    pub reset_file: &'static str,
    pub reset_project: &'static str,
    pub reset_milestone: &'static str,
    pub yes_no: &'static str,
    pub lang_switched: &'static str,
    pub lang_current: &'static str,
    pub visuals_none: &'static str,
    pub visuals_menu: &'static str,
    pub visual_opened: &'static str,
    pub open_failed: &'static str,

    pub tag_quiz: &'static str,
    pub tag_project: &'static str,
    pub tag_milestone: &'static str,
    pub baseline_offer_watch: &'static str,
    pub baseline_offer_cli: &'static str,
    pub journey_ahead_watch: &'static str,
    pub journey_ahead_cli: &'static str,
    pub baseline_done: &'static str,
    pub baseline_empty: &'static str,
    pub baseline_missing: &'static str,
    pub journey_missing: &'static str,
    pub arch_failed: &'static str,
    pub journey_status: &'static str,

    pub list_hint_move: &'static str,
    pub list_hint_open: &'static str,
    pub list_hint_fold: &'static str,
    pub list_hint_reset: &'static str,
    pub list_hint_search: &'static str,
    pub list_hint_tracks: &'static str,
    pub list_hint_filter: &'static str,
    pub list_hint_visuals: &'static str,
    pub list_hint_quit: &'static str,
    pub filter_done_on: &'static str,
    pub filter_pending_on: &'static str,
    pub filter_off: &'static str,
    pub track_all: &'static str,
    pub search: &'static str,
    pub not_found: &'static str,
    pub nothing_selected: &'static str,
    pub reset_ask: &'static str,
    pub done_word: &'static str,
    pub coming_soon: &'static str,
    pub pending_word: &'static str,

    pub tracks_header: &'static str,
    pub track_switched: &'static str,

    pub init_dir_exists: &'static str,
    pub init_in_initialized: &'static str,
    pub clippy_missing: &'static str,
    pub init_will_create: &'static str,
    pub init_done: &'static str,
    pub post_init: &'static str,
    pub lang_prompt: &'static str,
    pub learner_readme: &'static str,
    pub solutions_readme: &'static str,

    pub migrate_start: &'static str,
    pub migrate_backup: &'static str,
    pub migrate_done: &'static str,
    pub migrate_replaced: &'static str,
    pub not_old_layout: &'static str,

    pub question_cli: &'static str,
    pub question: &'static str,
    pub question_single: &'static str,
    pub question_multi: &'static str,
    pub question_text: &'static str,
    pub your_answer: &'static str,
    pub answer_right: &'static str,
    pub answer_wrong: &'static str,
    pub key_answer: &'static str,
    pub key_submit: &'static str,
    pub key_cancel: &'static str,
    pub not_question: &'static str,
    pub hint_step: &'static str,
    pub exam_no_hints: &'static str,
    pub tag_exam: &'static str,
    pub tag_outdated: &'static str,
    pub tag_question: &'static str,
    pub style_fix: &'static str,
    pub style_todo: &'static str,
    pub style_scratch: &'static str,
    pub style_bughunt: &'static str,
    pub key_diff: &'static str,
    pub diff_title: &'static str,
    pub diff_same: &'static str,
    pub diff_files: &'static str,
    pub diff_only_reference: &'static str,
    pub diff_only_yours: &'static str,
    pub diff_changed: &'static str,
    pub diff_no_reference: &'static str,
    pub diff_after_solve: &'static str,
    pub why: &'static str,
    pub milestone_done: &'static str,
    pub review_none: &'static str,
    pub review_created: &'static str,
    pub review_status: &'static str,
    pub review_done: &'static str,
    pub review_reminder: &'static str,
    pub review_active: &'static str,
    pub doctor_title: &'static str,
    pub doctor_all_good: &'static str,
    pub doctor_problems: &'static str,
    pub doc_too_old: &'static str,
    pub doc_missing: &'static str,
    pub doc_rust_fix: &'static str,
    pub doc_clippy_fix: &'static str,
    pub doc_docker_fix: &'static str,
    pub doc_compose_fix: &'static str,
    pub doc_service_down: &'static str,
    pub doc_protoc_fix: &'static str,
    pub doc_make_fix: &'static str,
    pub doc_disk: &'static str,
    pub doc_disk_low: &'static str,
    pub finish_line: &'static str,
    pub dev_check_missing: &'static str,
    pub mode_header: &'static str,
    pub mode_story: &'static str,
    pub mode_story_about: &'static str,
    pub mode_social: &'static str,
    pub mode_social_about: &'static str,
    pub mode_sql: &'static str,
    pub mode_sql_about: &'static str,
    pub mode_cql: &'static str,
    pub mode_cql_about: &'static str,
    pub mode_next: &'static str,
    pub mode_keys: &'static str,
    pub mode_back: &'static str,
    pub mode_empty: &'static str,
    pub mode_finished: &'static str,
    pub mode_current: &'static str,
    pub mode_switched: &'static str,
    pub mode_unknown: &'static str,
    pub key_mode: &'static str,
    pub prereq_need: &'static str,
    pub prereq_all_done: &'static str,
    pub prereq_topics: &'static str,
    pub prereq_upto: &'static str,
}

const PRE_INIT_BANNER_EN: &str = r"
       Welcome to...
                 _   _ _
  _ __ _   _ ___| |_| (_)_ __   __ _ ___
 | '__| | | / __| __| | | '_ \ / _` / __|
 | |  | |_| \__ \ |_| | | | | | (_| \__ \
 |_|   \__,_|___/\__|_|_|_| |_|\__, |___/
                               |___/

The course files couldn't be found in the current directory.
If you are just starting with Rustlings, run the command `rustlings init` to initialize it.";

const PRE_INIT_BANNER_RU: &str = r"
       Добро пожаловать в...
                 _   _ _
  _ __ _   _ ___| |_| (_)_ __   __ _ ___
 | '__| | | / __| __| | | '_ \ / _` / __|
 | |  | |_| \__ \ |_| | | | | | (_| \__ \
 |_|   \__,_|___/\__|_|_|_| |_|\__, |___/
                               |___/

В этой папке нет файлов курса.
Если ты только начинаешь, запусти `rustlings init`, он всё подготовит.";

pub static EN: Strings = Strings {
    pre_init: PRE_INIT_BANNER_EN,
    press_enter: "Press ENTER to continue ",
    need_tty: "Unsupported or missing terminal/TTY",
    format_too_new: "The course format is newer than this version of Rustlings supports.\nUpdate Rustlings first.",
    old_layout: "This folder uses the old Rustlings layout.\nRun `rustlings migrate` to move your progress into the new course (a backup is made first).",
    migrate_prompt: "Migrate this folder now? A backup is made first. (y/n) ",
    no_exercise: "No exercise named `{}`",
    unknown_track: "No track `{}`. Available tracks: {}",
    unknown_locale: "Unknown language `{}`. Available: en, ru",
    not_milestone: "`{}` is not a milestone",

    ran_with_errors: "Ran {} with errors",
    ran_ok: "✓ Successfully ran {}",
    skipped: "Couldn't check {} right now",
    solution_for_comparison: "Solution for comparison: ",
    next_exercise: "Next exercise: ",
    one_pending: "One exercise pending: ",
    many_pending: "{}/{} exercises pending. The first: ",
    skipped_in_check_all: "Not checked (milestones or services down): {}",
    reset_done: "The exercise {} has been reset",
    current_exercise: "Current exercise: ",
    hint: "Hint",
    no_hint: "No hint for this one.",
    checking_all: "Checking all exercises…",
    check_done: "   DONE ",
    check_pending: "PENDING ",
    check_skip: "   SKIP ",
    progress: "Progress",

    cargo_missing: "Failed to run `cargo`.\nDid you already install Rust?\nTry running `cargo --version` to diagnose the problem.",
    run_failed: "Failed to run `{}`",
    timed_out: "`{}` timed out after {} seconds",
    nonzero_exit: "The exercise didn't run successfully (nonzero exit code)",
    output: "Output",
    service_down: "The service `{}` is not reachable at {}.\nStart it with: `docker compose up -d {}`",
    checking: "Checking the exercise `{}`. Please wait…",
    project_missing: "The project folder {} was missing, it has been recreated from the original.",

    exercise_done: "Exercise done ✓",
    when_done: "When done experimenting, enter `n` to move on to the next exercise 🦀",
    track: "Track",
    chapter: "Chapter",
    exercise: "Exercise",
    file: "File",
    folder: "Folder",
    task: "Task",
    theory: "Theory",
    key_next: "next",
    key_run: "run",
    key_hint: "hint",
    key_list: "list",
    key_reset: "reset",
    key_quit: "quit",
    key_lang: "lang",
    key_visuals: "visuals",
    key_baseline: "baseline",
    quit_msg: "
We hope you're enjoying learning Rust!
If you want to continue working on the exercises at a later point, you can simply run `rustlings` again in this directory.
",
    notify_err: "
The automatic detection of exercise file changes failed :(
Please try running `rustlings` again.

If you keep getting this error, run `rustlings --manual-run` to deactivate the file watcher.
You need to manually trigger running the current exercise using `r` then.
",
    reset_file: "Resetting will undo all your changes to the file {}",
    reset_project: "Resetting will recreate the folder {} from scratch. All your changes there will be lost.",
    reset_milestone: "Resetting moves the project folder {} to a backup and starts it again from the reference solution of the previous milestone.",
    yes_no: "Reset (y/n)? ",
    lang_switched: "Language: {}. Re-created {} file(s), merged your edits into {} file(s).",
    lang_current: "Current language: {}. Switch with `rustlings lang en` or `rustlings lang ru`.",
    visuals_none: "This chapter has no visualizations.",
    visuals_menu: "Visualizations (press a number, any other key to cancel):",
    visual_opened: "Opened {}",
    open_failed: "Failed to open {}: {}",

    tag_quiz: "[quiz]",
    tag_project: "[project]",
    tag_milestone: "[milestone]",
    baseline_offer_watch: "Your project {} isn't ready for this milestone: it's missing or earlier milestones aren't done yet.\nPress `b` to start from the reference solution of `{}` (your current folder goes to a backup).",
    baseline_offer_cli: "Your project {} isn't ready for this milestone: it's missing or earlier milestones aren't done yet.\nRun `rustlings journey baseline {}` to start from the reference solution of `{}` (your current folder goes to a backup).",
    journey_ahead_watch: "Your project {} has already moved on to the milestone `{}`, past this one. The tests of this milestone can't run there: the code is different by now.\nTo do this milestone yourself, press `x`: the folder goes to a backup and the project starts again from the right point.",
    journey_ahead_cli: "Your project {} has already moved on to the milestone `{}`, past this one. The tests of this milestone can't run there: the code is different by now.\nTo do this milestone yourself, run `rustlings reset {}`: the folder goes to a backup and the project starts again from the right point.",
    baseline_done: "The project {} now starts from the reference solution of `{}`.",
    baseline_empty: "The project {} now starts from an empty folder (this is the first milestone).",
    baseline_missing: "The reference solution of `{}` is not part of this course version yet.",
    journey_missing: "The project folder {} doesn't exist yet. Open the first milestone or start from a reference solution.",
    arch_failed: "Architecture check failed:",
    journey_status: "{}: {}/{} milestones",

    list_hint_move: "move",
    list_hint_open: "continue at",
    list_hint_fold: "fold",
    list_hint_reset: "reset",
    list_hint_search: "search",
    list_hint_tracks: "track",
    list_hint_filter: "filter done/pending",
    list_hint_visuals: "visuals",
    list_hint_quit: "quit list",
    filter_done_on: "Showing solved exercises only. Press d again to show all",
    filter_pending_on: "Showing pending exercises only. Press p again to show all",
    filter_off: "Filter disabled",
    track_all: "all tracks",
    search: "search: ",
    not_found: " (not found)",
    nothing_selected: "Nothing selected",
    reset_ask: "Reset `{}`? All your changes will be lost. (y/n)",
    done_word: "done",
    coming_soon: "coming soon",
    pending_word: "pending",

    tracks_header: "Tracks (id, title, progress):",
    track_switched: "Track `{}`. Current exercise: ",

    init_dir_exists: "A directory with the name `rustlings` already exists in the current directory.
You probably already initialized Rustlings.
Run `cd rustlings`
Then run `rustlings` again",
    init_in_initialized: "It looks like Rustlings is already initialized in this directory.

If you already initialized Rustlings, run the command `rustlings` for instructions on getting started with the exercises.
Otherwise, please run `rustlings init` again in a different directory.",
    clippy_missing: "Clippy, the official Rust linter, is missing.\nPlease install it first before initializing Rustlings.",
    init_will_create: "This command will create the directory `rustlings/` which will contain the exercises.\nPress ENTER to continue ",
    init_done: "Initialization done ✓",
    post_init: "Run `cd rustlings` to go into the generated directory.\nThen run `rustlings` to get started.\n",
    lang_prompt: "Language / Язык [en/ru] (default {}): ",
    learner_readme: "# Rustlings

This is your space to solve the course exercises.
Run `rustlings` in this directory to get started, `rustlings list` to see the whole course
and `rustlings lang ru` to switch the language.

Services for the database and messaging exercises: `docker compose up -d`.
",
    solutions_readme: "# Solutions

A solution appears here after you finish the exercise. Compare it with yours:
there is almost always more than one good way to solve a task.
",

    migrate_start: "Migrating the old Rustlings folder into the new course…",
    migrate_backup: "Backup of your old files: {}",
    migrate_done: "Migration done. Solved exercises kept: {}. Current exercise: ",
    migrate_replaced: "These exercises changed in the new course version, they start fresh (your old files are in the backup): {}",
    not_old_layout: "This folder doesn't look like an old Rustlings folder, nothing to migrate.",

    question_cli: "Answer in the watch mode (`rustlings`) or with `rustlings answer {} <answer>`.",
    question: "Question",
    question_single: "Press the number of the right answer.",
    question_multi: "Several answers are right: mark them with numbers and press Enter.",
    question_text: "Press `a` and type your answer, Enter sends it, Esc cancels.",
    your_answer: "Your answer: ",
    answer_right: "Right!",
    answer_wrong: "Not quite. Think again and try once more.",
    key_answer: "answer",
    key_submit: "send",
    key_cancel: "cancel",
    not_question: "`{}` is not a question",
    hint_step: "Hint {} of {}",
    exam_no_hints: "This is an exam: no hints this time, you've got this.",
    tag_exam: "[exam]",
    tag_outdated: "[new version: rustlings reset]",
    tag_question: "[question]",
    style_fix: "[fix]",
    style_todo: "[todo]",
    style_scratch: "[from scratch]",
    style_bughunt: "[bug hunt]",
    key_diff: "diff",
    diff_title: "Your code (-) and the reference (+): {}",
    diff_same: "Your code matches the reference solution.",
    diff_files: "Files that differ (press a number, `d` shows the next one):",
    diff_only_reference: "only in the reference",
    diff_only_yours: "only in your code",
    diff_changed: "differs",
    diff_no_reference: "There is no reference solution for this exercise.",
    diff_after_solve: "You can compare with the reference after you solve the exercise.",
    why: "Why the reference looks like this",
    milestone_done: "Milestone done: {}",
    review_none: "Nothing to review yet: solve more exercises and come back in a few days.",
    review_created: "Review of {}: solve these exercises again from scratch, then run `rustlings review` to check them.",
    review_status: "Review of {}: {} of {} solved.",
    review_done: "Review complete, nice work! Your main progress didn't change.",
    review_reminder: "There are exercises to review: `rustlings review`.",
    review_active: "A review is in progress: `rustlings review` checks it.",
    doctor_title: "Checking your environment (no network needed):",
    doctor_all_good: "Everything looks good.",
    doctor_problems: "Problems found: {}. Fix them and run `rustlings doctor` again.",
    doc_too_old: "{} {} is older than 1.88: update with `rustup update stable`.",
    doc_missing: "`{}` was not found.",
    doc_rust_fix: "Install Rust with rustup, then run `rustup update stable`.",
    doc_clippy_fix: "Install it with `rustup component add clippy`.",
    doc_docker_fix: "Install Docker Desktop or Docker Engine: the database and NATS exercises run their services in containers.",
    doc_compose_fix: "Install the Docker Compose plugin, `docker compose version` must work.",
    doc_service_down: "{} is not reachable at {}: run `docker compose up -d {}` in the course folder.",
    doc_protoc_fix: "protoc is needed for the gRPC chapters: install the protobuf compiler (`brew install protobuf` or `apt install protobuf-compiler`).",
    doc_make_fix: "Optional: cargo-make starts several services with one command (`cargo install cargo-make`).",
    doc_disk: "Free disk space: {} GB",
    doc_disk_low: "Builds of tokio, tonic and sqlx take several GB: free up at least {} GB.",
    finish_line: "You made it to the finish line!",
    dev_check_missing: "`course/tools/check.py` was not found. Run `rustlings dev check` in the root of the course repository.",
    mode_header: "Rustlings: what are we doing?",
    mode_story: "Story practice",
    mode_story_about: "The whole course in order: chapters, projects, quizzes and the social network milestones between them.",
    mode_social: "Building the social network",
    mode_social_about: "Only the milestones of the end-to-end project. Each one shows which tracks and topics to go through first.",
    mode_sql: "SQL only",
    mode_sql_about: "Only the PostgreSQL chapters: queries, schema, transactions, sqlx, indexes, migrations.",
    mode_cql: "CQL only (ScyllaDB)",
    mode_cql_about: "Only the ScyllaDB chapters: CQL, query-first modeling, the Rust driver.",
    mode_next: "Next milestone: {}, what it needs is {}% done",
    mode_keys: "↑↓ or 1..4: choose · Enter: start · q: {}",
    mode_back: "back",
    mode_empty: "There are no exercises in this mode yet, they are still being written.",
    mode_finished: "Everything in «{}» is done! Press m to pick another mode.",
    mode_current: "Modes (id, title, progress), m in the watch mode switches them:",
    mode_switched: "Mode: {}",
    mode_unknown: "There is no mode `{}`. Modes: {}",
    key_mode: "mode",
    prereq_need: "Before this milestone, go through these (done {}%):",
    prereq_all_done: "Everything this milestone needs is done, go ahead!",
    prereq_topics: "New topics since the previous milestone:",
    prereq_upto: "{} (up to chapter {})",
};

pub static RU: Strings = Strings {
    pre_init: PRE_INIT_BANNER_RU,
    press_enter: "Нажми ENTER, чтобы продолжить ",
    need_tty: "Нужен настоящий терминал (TTY), а его нет",
    format_too_new: "Формат курса новее, чем умеет эта версия Rustlings.\nСначала обнови Rustlings.",
    old_layout: "Эта папка устроена по старой схеме Rustlings.\nЗапусти `rustlings migrate`, и прогресс переедет в новый курс (сначала будет сделан бэкап).",
    migrate_prompt: "Перенести эту папку сейчас? Сначала будет бэкап. (y/n) ",
    no_exercise: "Задачи `{}` в курсе нет",
    unknown_track: "Раздела `{}` нет. Есть такие: {}",
    unknown_locale: "Не знаю язык `{}`. Есть en и ru",
    not_milestone: "`{}` не веха",

    ran_with_errors: "{} пока с ошибками",
    ran_ok: "✓ Получилось: {}",
    skipped: "Проверить {} сейчас не получится",
    solution_for_comparison: "Решение для сравнения: ",
    next_exercise: "Следующая задача: ",
    one_pending: "Осталась одна задача: ",
    many_pending: "Не решено {} из {}. Первая из них: ",
    skipped_in_check_all: "Не проверялись (вехи или выключенные сервисы): {}",
    reset_done: "Задача {} сброшена к исходному виду",
    current_exercise: "Текущая задача: ",
    hint: "Подсказка",
    no_hint: "Тут без подсказки, справишься.",
    checking_all: "Проверяю все задачи…",
    check_done: " ГОТОВО ",
    check_pending: "  ЖДЁТ ",
    check_skip: " ПРОПУСК ",
    progress: "Прогресс",

    cargo_missing: "Не получилось запустить `cargo`.\nRust точно установлен?\nПопробуй `cargo --version`, чтобы понять, в чём дело.",
    run_failed: "Не получилось запустить `{}`",
    timed_out: "`{}` не уложился в {} с и был остановлен",
    nonzero_exit: "Программа завершилась с ненулевым кодом выхода",
    output: "Вывод",
    service_down: "Сервис `{}` не отвечает на {}.\nПодними его: `docker compose up -d {}`",
    checking: "Проверяю задачу `{}`, подожди…",
    project_missing: "Папки проекта {} не было, я создал её заново из оригинала.",

    exercise_done: "Задача решена ✓",
    when_done: "Наигрался? Жми `n`, пойдём к следующей задаче 🦀",
    track: "Раздел",
    chapter: "Глава",
    exercise: "Задача",
    file: "Файл",
    folder: "Папка",
    task: "ТЗ",
    theory: "Теория",
    key_next: "дальше",
    key_run: "запуск",
    key_hint: "подсказка",
    key_list: "список",
    key_reset: "сброс",
    key_quit: "выход",
    key_lang: "язык",
    key_visuals: "схемы",
    key_baseline: "эталон",
    quit_msg: "
Надеемся, тебе нравится учить Rust!
Чтобы продолжить потом, просто снова запусти `rustlings` в этой папке.
",
    notify_err: "
Не получилось следить за изменениями файлов :(
Попробуй запустить `rustlings` ещё раз.

Если ошибка не уходит, запусти `rustlings --manual-run`: слежение выключится,
а проверку текущей задачи будешь запускать сам клавишей `r`.
",
    reset_file: "Сброс отменит все твои изменения в файле {}",
    reset_project: "Сброс создаст папку {} заново. Все твои изменения в ней пропадут.",
    reset_milestone: "Сброс уберёт папку проекта {} в бэкап и начнёт её заново с эталона прошлой вехи.",
    yes_no: "Сбросить (y/n)? ",
    lang_switched: "Язык: {}. Пересоздано файлов: {}, твои правки сохранены в {}.",
    lang_current: "Сейчас язык: {}. Переключить: `rustlings lang en` или `rustlings lang ru`.",
    visuals_none: "В этой главе нет анимированных схем.",
    visuals_menu: "Схемы главы (нажми цифру, любая другая клавиша отменит):",
    visual_opened: "Открыл {}",
    open_failed: "Не получилось открыть {}: {}",

    tag_quiz: "[квиз]",
    tag_project: "[проект]",
    tag_milestone: "[веха]",
    baseline_offer_watch: "Проект {} пока не готов к этой вехе: его нет или прошлые вехи не пройдены.\nНажми `b`, чтобы начать с эталона вехи `{}` (текущая папка уйдёт в бэкап).",
    baseline_offer_cli: "Проект {} пока не готов к этой вехе: его нет или прошлые вехи не пройдены.\nЗапусти `rustlings journey baseline {}`, чтобы начать с эталона вехи `{}` (текущая папка уйдёт в бэкап).",
    journey_ahead_watch: "Проект {} уже дошёл до вехи `{}`, это дальше текущей. Тесты этой вехи там не запустить: код уже другой.\nЧтобы пройти её самому, нажми `x`: папка уйдёт в бэкап, и проект начнётся заново с нужной точки.",
    journey_ahead_cli: "Проект {} уже дошёл до вехи `{}`, это дальше текущей. Тесты этой вехи там не запустить: код уже другой.\nЧтобы пройти её самому, запусти `rustlings reset {}`: папка уйдёт в бэкап, и проект начнётся заново с нужной точки.",
    baseline_done: "Проект {} теперь начинается с эталона вехи `{}`.",
    baseline_empty: "Проект {} теперь начинается с пустой папки (это первая веха).",
    baseline_missing: "Эталона вехи `{}` в этой версии курса ещё нет.",
    journey_missing: "Папки проекта {} пока нет. Открой первую веху или начни с эталона.",
    arch_failed: "Архитектура не сходится:",
    journey_status: "{}: вех {} из {}",

    list_hint_move: "ходить",
    list_hint_open: "открыть",
    list_hint_fold: "свернуть",
    list_hint_reset: "сброс",
    list_hint_search: "поиск",
    list_hint_tracks: "раздел",
    list_hint_filter: "решённые/нерешённые",
    list_hint_visuals: "схемы",
    list_hint_quit: "выйти",
    filter_done_on: "Показываю только решённые. Нажми d ещё раз, чтобы видеть все",
    filter_pending_on: "Показываю только нерешённые. Нажми p ещё раз, чтобы видеть все",
    filter_off: "Фильтр выключен",
    track_all: "все разделы",
    search: "поиск: ",
    not_found: " (не нашёл)",
    nothing_selected: "Ничего не выбрано",
    reset_ask: "Сбросить `{}`? Все твои изменения пропадут. (y/n)",
    done_word: "решено",
    coming_soon: "скоро будет",
    pending_word: "не решено",

    tracks_header: "Разделы (id, название, прогресс):",
    track_switched: "Раздел `{}`. Текущая задача: ",

    init_dir_exists: "Папка `rustlings` здесь уже есть.
Похоже, Rustlings уже установлен.
Перейди в неё: `cd rustlings`
И запусти `rustlings`",
    init_in_initialized: "Похоже, Rustlings уже установлен в этой папке.

Если это так, просто запусти `rustlings`.
Если нет, запусти `rustlings init` в другой папке.",
    clippy_missing: "Не нашёл Clippy, официальный линтер Rust.\nУстанови его, а потом запусти `rustlings init` снова.",
    init_will_create: "Сейчас появится папка `rustlings/` со всеми задачами курса.\nНажми ENTER, чтобы продолжить ",
    init_done: "Всё готово ✓",
    post_init: "Перейди в папку: `cd rustlings`\nИ запусти `rustlings`, чтобы начать.\n",
    lang_prompt: "Language / Язык [en/ru] (по умолчанию {}): ",
    learner_readme: "# Rustlings

Здесь ты решаешь задачи курса.
Запусти `rustlings` в этой папке, чтобы начать, `rustlings list`, чтобы увидеть весь курс,
и `rustlings lang en`, чтобы переключить язык.

Сервисы для задач с базами данных и NATS: `docker compose up -d`.
",
    solutions_readme: "# Решения

Решение задачи появляется здесь, когда ты её решишь. Сравни со своим:
хороших способов почти всегда больше одного.
",

    migrate_start: "Переношу старую папку Rustlings в новый курс…",
    migrate_backup: "Бэкап старых файлов: {}",
    migrate_done: "Готово. Решённых задач сохранено: {}. Текущая задача: ",
    migrate_replaced: "Эти задачи в новой версии курса другие, они начинаются заново (старые файлы лежат в бэкапе): {}",
    not_old_layout: "Эта папка не похожа на старую папку Rustlings, переносить нечего.",

    question_cli: "Ответь в режиме наблюдения (`rustlings`) или командой `rustlings answer {} <ответ>`.",
    question: "Вопрос",
    question_single: "Нажми номер правильного ответа.",
    question_multi: "Верных ответов несколько: отметь их цифрами и нажми Enter.",
    question_text: "Нажми `a` и напиши ответ, Enter отправит его, Esc отменит.",
    your_answer: "Твой ответ: ",
    answer_right: "Верно!",
    answer_wrong: "Не совсем. Подумай ещё и попробуй снова.",
    key_answer: "ответить",
    key_submit: "отправить",
    key_cancel: "отмена",
    not_question: "`{}` не вопрос",
    hint_step: "Подсказка {} из {}",
    exam_no_hints: "Это экзамен: подсказок нет, ты справишься.",
    tag_exam: "[экзамен]",
    tag_outdated: "[есть новая версия: rustlings reset]",
    tag_question: "[вопрос]",
    style_fix: "[почини]",
    style_todo: "[допиши]",
    style_scratch: "[с нуля]",
    style_bughunt: "[найди баг]",
    key_diff: "сравнить",
    diff_title: "Твой код (-) и эталон (+): {}",
    diff_same: "Твой код совпадает с эталоном.",
    diff_files: "Отличаются файлы (нажми цифру, `d` покажет следующий):",
    diff_only_reference: "есть только в эталоне",
    diff_only_yours: "есть только у тебя",
    diff_changed: "отличается",
    diff_no_reference: "Для этой задачи нет эталона.",
    diff_after_solve: "Сравнить с эталоном можно, когда задача решена.",
    why: "Почему эталон написан так",
    milestone_done: "Веха пройдена: {}",
    review_none: "Повторять пока нечего: реши ещё задачи и возвращайся через пару дней.",
    review_created: "Повторение от {}: реши эти задачи заново и запусти `rustlings review`, чтобы проверить их.",
    review_status: "Повторение от {}: решено {} из {}.",
    review_done: "Повторение пройдено, отлично! Основной прогресс не менялся.",
    review_reminder: "Есть задачи на повторение: `rustlings review`.",
    review_active: "Повторение ещё идёт: `rustlings review` проверит его.",
    doctor_title: "Проверяю окружение (сеть не нужна):",
    doctor_all_good: "Всё в порядке.",
    doctor_problems: "Нашлось проблем: {}. Исправь их и запусти `rustlings doctor` ещё раз.",
    doc_too_old: "{} {} старше 1.88: обнови через `rustup update stable`.",
    doc_missing: "`{}` не найден.",
    doc_rust_fix: "Поставь Rust через rustup и выполни `rustup update stable`.",
    doc_clippy_fix: "Поставь его: `rustup component add clippy`.",
    doc_docker_fix: "Поставь Docker Desktop или Docker Engine: задачи с базами и NATS запускают сервисы в контейнерах.",
    doc_compose_fix: "Поставь плагин Docker Compose, должна работать команда `docker compose version`.",
    doc_service_down: "{} не отвечает на {}: запусти `docker compose up -d {}` в папке курса.",
    doc_protoc_fix: "protoc нужен для глав про gRPC: поставь компилятор protobuf (`brew install protobuf` или `apt install protobuf-compiler`).",
    doc_make_fix: "Необязательно: cargo-make запускает несколько сервисов одной командой (`cargo install cargo-make`).",
    doc_disk: "Свободно на диске: {} ГБ",
    doc_disk_low: "Сборки tokio, tonic и sqlx занимают несколько ГБ: освободи хотя бы {} ГБ.",
    finish_line: "Ты дошёл до финиша!",
    dev_check_missing: "Не нашёл `course/tools/check.py`. Запускай `rustlings dev check` в корне репозитория курса.",
    mode_header: "Rustlings: что будем делать?",
    mode_story: "Сюжетная практика",
    mode_story_about: "Весь курс по порядку: главы, проекты, квизы, а между ними вехи соцсети.",
    mode_social: "Создание соцсети",
    mode_social_about: "Только вехи сквозного проекта. У каждой видно, какие разделы и темы пройти перед ней.",
    mode_sql: "Чисто SQL",
    mode_sql_about: "Только главы про PostgreSQL: запросы, схема, транзакции, sqlx, индексы, миграции.",
    mode_cql: "Чисто CQL (ScyllaDB)",
    mode_cql_about: "Только главы про ScyllaDB: CQL, моделирование под запросы, драйвер на Rust.",
    mode_next: "Следующая веха: {}, нужное для неё пройдено на {}%",
    mode_keys: "↑↓ или 1..4: выбрать · Enter: начать · q: {}",
    mode_back: "назад",
    mode_empty: "В этом режиме пока нет задач, они ещё пишутся.",
    mode_finished: "Всё в режиме «{}» пройдено! Нажми m, чтобы выбрать другой режим.",
    mode_current: "Режимы (id, название, прогресс), в режиме задач их меняет клавиша m:",
    mode_switched: "Режим: {}",
    mode_unknown: "Нет режима `{}`. Есть такие: {}",
    key_mode: "режим",
    prereq_need: "Перед этой вехой стоит пройти (готово на {}%):",
    prereq_all_done: "Всё нужное для этой вехи пройдено, можно начинать!",
    prereq_topics: "Новые темы с прошлой вехи:",
    prereq_upto: "{} (до главы {})",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_replaces_in_order() {
        assert_eq!(fill("{} of {}", &[&1, &"x"]), "1 of x");
        assert_eq!(fill("no args", &[]), "no args");
        assert_eq!(fill("{} {}", &[&1]), "1 ");
    }

    #[test]
    fn russian_strings_have_no_dashes() {
        let texts = [
            RU.pre_init,
            RU.old_layout,
            RU.service_down,
            RU.baseline_offer_watch,
            RU.baseline_offer_cli,
            RU.journey_ahead_watch,
            RU.journey_ahead_cli,
            RU.learner_readme,
            RU.solutions_readme,
            RU.quit_msg,
            RU.notify_err,
            RU.when_done,
        ];
        for text in texts {
            assert!(!text.contains('—') && !text.contains('–') && !text.contains(" -- "));
        }
    }
}
