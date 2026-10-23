use model::Event;

fn main() {
    let header = serde_json::json!({ "zone": "eu", "id": 7, "name": "nightly" });
    println!("{header}");
    println!("{}", store::encode(&[Event::new(1, "login")]));
}
