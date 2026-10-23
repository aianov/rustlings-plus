// Продолжаем прошлое упражнение: нужно, чтобы все потоки завершили свою
// работу. Но теперь запущенные потоки сами отвечают за обновление общего
// значения: `JobStatus.jobs_done`

use std::{sync::Arc, thread, time::Duration};

struct JobStatus {
    jobs_done: u32,
}

fn main() {
    // TODO: Одного `Arc` недостаточно, если нужно **изменяемое** общее состояние.
    let status = Arc::new(JobStatus { jobs_done: 0 });

    let mut handles = Vec::new();
    for _ in 0..10 {
        let status_shared = Arc::clone(&status);
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(250));

            // TODO: Прежде чем обновлять общее значение, нужно кое-что сделать.
            status_shared.jobs_done += 1;
        });
        handles.push(handle);
    }

    // Ждём завершения всех задач.
    for handle in handles {
        handle.join().unwrap();
    }

    // TODO: Выведи значение `JobStatus.jobs_done`.
    println!("Выполнено задач: {}", todo!());
}
