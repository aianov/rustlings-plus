#[derive(Debug)]
enum Message {
    // TODO: Определи несколько видов сообщений, как они используются ниже.
}

fn main() {
    println!("{:?}", Message::Resize);
    println!("{:?}", Message::Move);
    println!("{:?}", Message::Echo);
    println!("{:?}", Message::ChangeColor);
    println!("{:?}", Message::Quit);
}
