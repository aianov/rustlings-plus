// Этот квиз проверяет:
// - Обобщения
// - Трейты
//
// В воображаемой волшебной школе появилась новая система выпуска табелей,
// написанная на Rust! Сейчас она умеет создавать только табели с числовой
// оценкой (например, 1.0 -> 5.5). Но школа выставляет и буквенные оценки
// (A+ -> F-), и печатать нужно оба вида
// табелей!
//
// Внеси нужные изменения в структуру `ReportCard` и блок impl, чтобы
// поддержать буквенные табели в дополнение к числовым.

// TODO: Измени структуру, как описано выше.
struct ReportCard {
    grade: f32,
    student_name: String,
    student_age: u8,
}

// TODO: Измени блок impl, как описано выше.
impl ReportCard {
    fn print(&self) -> String {
        format!(
            "{} ({}) - achieved a grade of {}",
            self.student_name, self.student_age, self.grade,
        )
    }
}

fn main() {
    // Здесь можно поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_numeric_report_card() {
        let report_card = ReportCard {
            grade: 2.1,
            student_name: "Tom Wriggle".to_string(),
            student_age: 12,
        };
        assert_eq!(
            report_card.print(),
            "Tom Wriggle (12) - achieved a grade of 2.1",
        );
    }

    #[test]
    fn generate_alphabetic_report_card() {
        let report_card = ReportCard {
            grade: "A+",
            student_name: "Gary Plotter".to_string(),
            student_age: 11,
        };
        assert_eq!(
            report_card.print(),
            "Gary Plotter (11) - achieved a grade of A+",
        );
    }
}
