// Алиса учительница начальных классов, и ей нужно посчитать средний балл за
// тест в трёх классах, которые она ведёт. Вместо того чтобы считать по очереди,
// она решает попросить о помощи друзей Боба и Екатерину. Работая вместе, они
// справятся гораздо быстрее.
//
// Давай смоделируем это с помощью асинхронного программирования. Каждый человек
// представлен асинхронной задачей, которая может выполняться конкурентно.

// Асинхронные задачи должны выполняться в «рантайме», которого в стандартной
// библиотеке Rust нет. Здесь мы используем популярный рантайм `tokio`.
// Макрос `tokio::main` оборачивает всю функцию main в рантайм.
#[tokio::main]
async fn main() {
    let mean_score_a = tokio::spawn(calculate_mean_score("scores_class_a.txt"));
    let mean_score_b = tokio::spawn(calculate_mean_score("scores_class_b.txt"));
    let mean_score_c = tokio::spawn(calculate_mean_score("scores_class_c.txt"));

    assert_eq!(mean_score_a.await.unwrap(), 84); // алиса
    assert_eq!(mean_score_b.await.unwrap(), 89); // боб
    assert_eq!(mean_score_c.await.unwrap(), 76); // екатерина
}

async fn calculate_mean_score(scores_file: &str) -> usize {
    // Читаем файл асинхронно
    let file = tokio::fs::read_to_string(scores_file).await.unwrap();

    // Инициализируем сумму и число оценок
    let mut sum = 0;
    let mut n = 0;
    for line in file.lines() {
        // Разбираем каждую строку как оценку
        let score = line.parse::<usize>().unwrap();
        sum += score;
        n += 1;
    }

    sum / n
}
