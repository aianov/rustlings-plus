pub mod engine {
    mod render {
        pub mod mesh {
            #[derive(Debug, PartialEq)]
            pub struct Mesh {
                pub indices: Vec<u32>,
            }

            pub(crate) fn build_indices(vertices: u32) -> Vec<u32> {
                (0..vertices).collect()
            }

            impl Mesh {
                pub fn quad() -> Mesh {
                    Mesh { indices: build_indices(4) }
                }
            }
        }

        pub mod shader {
            #[derive(Debug, PartialEq)]
            pub struct Shader {
                pub name: &'static str,
            }

            impl Shader {
                pub fn flat() -> Shader {
                    Shader { name: "flat" }
                }
            }
        }
    }

    mod physics {
        pub mod collider {
            #[derive(Debug, PartialEq)]
            pub struct Collider {
                pub radius: u32,
            }
        }

        pub mod world {
            use super::collider::Collider;

            #[derive(Debug, Default)]
            pub struct World {
                pub bodies: Vec<Collider>,
            }

            impl World {
                pub fn spawn(&mut self, radius: u32) {
                    self.bodies.push(Collider { radius });
                }
            }
        }
    }

    pub use self::physics::collider::Collider;
    pub use self::physics::world::World;
    pub use self::render::mesh::Mesh;
    pub use self::render::shader::Shader;

    pub mod prelude {
        pub use super::{Collider, Mesh, Shader, World};
    }
}

fn main() {
    let mut world = engine::World::default();
    world.spawn(3);
    println!("{} {:?}", world.bodies.len(), engine::Mesh::quad());
}

#[cfg(test)]
mod tests {
    use super::*;

const SRC: &str = include_str!("wsp5.rs");

fn rustc(args: &[&str]) -> Result<(), String> {
    let out = std::process::Command::new("rustc")
        .args(args)
        .output()
        .expect("не удалось запустить rustc: он должен быть в PATH");
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

fn scratch_dir() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rustlings_probe_{}_{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).expect("не удалось создать временную папку");
    dir
}

fn inside(extra: &str) -> Result<(), String> {
    let dir = scratch_dir();
    let file = dir.join("probe.rs");
    std::fs::write(&file, format!("{SRC}\n{extra}\n")).expect("не удалось записать файл");
    let result = rustc(&[
        "--edition", "2024", "--crate-type", "lib", "--crate-name", "probe", "--emit=metadata",
        "-A", "warnings", "-D", "private_interfaces", "-D", "private_bounds",
        "--out-dir", dir.to_str().expect("путь не UTF-8"), file.to_str().expect("путь не UTF-8"),
    ]);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn outside(client: &str) -> Result<(), String> {
    let dir = scratch_dir();
    let lib = dir.join("exercise.rs");
    std::fs::write(&lib, SRC).expect("не удалось записать файл");
    let dir_s = dir.to_str().expect("путь не UTF-8");
    rustc(&[
        "--edition", "2024", "--crate-type", "lib", "--crate-name", "exercise", "-A", "warnings",
        "--out-dir", dir_s, lib.to_str().expect("путь не UTF-8"),
    ])
    .map_err(|e| format!("сам файл не собирается как библиотека:\n{e}"))?;
    let bin = dir.join("client.rs");
    std::fs::write(&bin, client).expect("не удалось записать файл");
    let rlib = dir.join("libexercise.rlib");
    let result = rustc(&[
        "--edition", "2024", "--crate-type", "lib", "--crate-name", "client", "--emit=metadata", "-A", "warnings",
        "--extern", &format!("exercise={}", rlib.to_str().expect("путь не UTF-8")),
        "--out-dir", dir_s, bin.to_str().expect("путь не UTF-8"),
    ]);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

    #[test]
    fn flat_names_work() {
        assert_eq!(engine::Mesh::quad().indices, vec![0, 1, 2, 3], "у квадрата четыре индекса");
        assert_eq!(engine::Shader::flat().name, "flat");
        let mut world = engine::World::default();
        world.spawn(2);
        assert_eq!(world.bodies, vec![engine::Collider { radius: 2 }], "в мире одно тело радиуса 2");
    }

    #[test]
    fn prelude_brings_the_main_names() {
        use crate::engine::prelude::*;
        let mut world = World::default();
        world.spawn(1);
        assert_eq!(world.bodies.len(), 1);
        assert_eq!(Mesh::quad().indices.len(), 4);
        assert_eq!(Shader::flat().name, "flat");
        assert_eq!(world.bodies[0], Collider { radius: 1 });
    }

    #[test]
    fn outside_crate_uses_only_the_facade() {
        assert!(
            outside("pub fn f() -> exercise::engine::Mesh { exercise::engine::Mesh::quad() }").is_ok(),
            "engine::Mesh должен быть доступен снаружи"
        );
        assert!(
            outside("use exercise::engine::prelude::*; pub fn f() -> (Mesh, Shader, World, Collider) { (Mesh::quad(), Shader::flat(), World::default(), Collider { radius: 1 }) }").is_ok(),
            "через prelude снаружи должны быть доступны Mesh, Shader, World и Collider"
        );
    }

    #[test]
    fn internal_modules_are_closed() {
        assert!(
            outside("pub fn f() -> exercise::engine::render::mesh::Mesh { exercise::engine::Mesh::quad() }").is_err(),
            "путь engine::render::mesh::Mesh всё ещё открыт снаружи: модуль render должен быть приватным"
        );
        assert!(
            outside("pub fn f() -> exercise::engine::physics::world::World { exercise::engine::World::default() }").is_err(),
            "путь engine::physics::world::World всё ещё открыт снаружи: модуль physics должен быть приватным"
        );
    }

    #[test]
    fn helpers_are_not_part_of_the_api() {
        assert!(
            outside("pub fn f() -> Vec<u32> { exercise::engine::prelude::build_indices(3) }").is_err(),
            "build_indices не должна попадать в prelude"
        );
        assert!(
            outside("pub fn f() -> Vec<u32> { exercise::engine::build_indices(3) }").is_err(),
            "build_indices не должна попадать в фасад"
        );
        assert!(
            inside("mod other { pub fn f() -> Vec<u32> { crate::engine::render::mesh::build_indices(3) } }").is_err(),
            "путь к build_indices не должен быть виден модулям вне engine: модуль render должен быть приватным"
        );
    }
}
