
pub fn transform(s: &str) -> Vec<u8> {
    hex::decode(s.trim()).expect("Decoding failed")
}