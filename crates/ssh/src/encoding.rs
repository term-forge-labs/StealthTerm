use encoding_rs::Encoding;

pub struct RemoteEncoding {
    encoding: &'static Encoding,
}

impl RemoteEncoding {
    pub fn new(name: &str) -> Self {
        let encoding = Encoding::for_label(name.as_bytes())
            .unwrap_or(encoding_rs::UTF_8);
        Self { encoding }
    }

    pub fn decode(&self, bytes: &[u8]) -> String {
        let (decoded, _, _) = self.encoding.decode(bytes);
        decoded.into_owned()
    }

    pub fn encode(&self, text: &str) -> Vec<u8> {
        let (encoded, _, _) = self.encoding.encode(text);
        encoded.into_owned().to_vec()
    }
}

impl Default for RemoteEncoding {
    fn default() -> Self {
        Self { encoding: encoding_rs::UTF_8 }
    }
}
