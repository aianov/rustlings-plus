// Продолжаем прошлое упражнение: нужно, чтобы все потоки завершили свою
// работу. Но теперь запущенные потоки сами отвечают за обновление общего
// значения: `JobStatus.jobs_done`

use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

struct JobStatus {
    jobs_done: u32,
}

fn main() {
    // Одного `Arc` недостаточно, если нужно **изменяемое** общее состояние.
    // Нужно обернуть значение в `Mutex`.
    let status = Arc::new(Mutex::new(JobStatus { jobs_done: 0 }));
    //                    ^^^^^^^^^^^                          ^

    let mut handles = Vec::new();
    for _ in 0..10 {
        let status_shared = Arc::clone(&status);
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(250));

            // Перед обновлением общего значения берём блокировку.
            status_shared.lock().unwrap().jobs_done += 1;
            //           ^^^^^^^^^^^^^^^^
        });
        handles.push(handle);
    }

    // Ждём завершения всех задач.
    for handle in handles {
        handle.join().unwrap();
    }

    println!("Выполнено задач: {}", status.lock().unwrap().jobs_done);
    //                        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
}
