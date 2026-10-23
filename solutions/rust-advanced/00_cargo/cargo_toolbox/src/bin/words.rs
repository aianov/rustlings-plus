fn main() {
    for word in textstat::stopwords() {
        println!("{word}");
    }
}
