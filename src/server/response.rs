use hyper::StatusCode;

pub struct Response {
    pub status: StatusCode,
    pub body: Vec<u8>,
    pub headers: Vec<(String, String)>,
}

impl Response {
    pub fn new(body: Vec<u8>) -> Self {
        Self { status: StatusCode::OK, body, headers: Vec::new() }
    }

    pub fn empty() -> Self {
        Self::new(Vec::new())
    }

    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    pub fn text(s: &str) -> Self {
        Self::new(s.as_bytes().to_vec())
    }

    pub fn json(json: &serde_json::Value) -> Self {
        let body = serde_json::to_vec(json).unwrap_or_default();
        Self::new(body).with_header("Content-Type", "application/json; charset=utf-8")
    }

    pub fn json_success(json: &serde_json::Value) -> Self {
        Self::json(json)
    }

    pub fn image(data: Vec<u8>, ext: &str) -> Self {
        let mime = match ext.trim_start_matches('.') {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            _ => "image/jpeg",
        };
        Self::new(data).with_header("Content-Type", mime)
    }

    pub fn redirect(location: &str) -> Self {
        Self::empty().with_status(StatusCode::MOVED_PERMANENTLY).with_header("Location", location)
    }

    pub fn not_found() -> Self {
        let json = serde_json::json!({"error": "not found"});
        Self::json(&json).with_status(StatusCode::NOT_FOUND)
    }

    pub fn into_hyper_response(self) -> hyper::Response<http_body_util::Full<bytes::Bytes>> {
        let mut builder = hyper::Response::builder().status(self.status);

        for (k, v) in &self.headers {
            builder = builder.header(k.as_str(), v.as_str());
        }

        if !self.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-length")) {
            builder = builder.header("Content-Length", self.body.len().to_string());
        }

        builder
            .body(http_body_util::Full::new(bytes::Bytes::from(self.body)))
            .unwrap_or_else(|_| hyper::Response::new(http_body_util::Full::new(bytes::Bytes::new())))
    }
}
