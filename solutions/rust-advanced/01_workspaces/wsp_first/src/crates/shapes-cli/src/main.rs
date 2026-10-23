use geometry::Rect;

fn main() {
    let args: Vec<f64> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    let [w, h] = args[..] else {
        eprintln!("usage: shapes-cli <width> <height>");
        return;
    };
    let rect = Rect::new(w, h);
    println!("area={} perimeter={}", rect.area(), rect.perimeter());
}
