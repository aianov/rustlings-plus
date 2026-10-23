use post_service::controller::Request;

fn main() {
    let controller = post_service::bootstrap::build();
    let resp = controller.handle(Request {
        method: "POST".to_string(),
        path: "/posts".to_string(),
        body: r#"{"author":1,"text":"привет"}"#.to_string(),
    });
    println!("{} {}", resp.status, resp.body);
}
