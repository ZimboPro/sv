/// HTTP methods
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Default, Clone)]
pub enum HttpMethod {
  #[default]
  Get,
  Post,
  Put,
  Delete,
  Patch,
  Head,
  Options,
  Trace,
  Connect,
  /// Any HTTP method is allowed
  Any,
}

impl From<String> for HttpMethod {
  fn from(s: String) -> Self {
    match s.to_lowercase().as_str() {
      "get" => HttpMethod::Get,
      "post" => HttpMethod::Post,
      "put" => HttpMethod::Put,
      "delete" => HttpMethod::Delete,
      "patch" => HttpMethod::Patch,
      "head" => HttpMethod::Head,
      "options" => HttpMethod::Options,
      "trace" => HttpMethod::Trace,
      "connect" => HttpMethod::Connect,
      "*" => HttpMethod::Any,
      _ => panic!("Invalid HTTP method"),
    }
  }
}

impl From<&str> for HttpMethod {
  fn from(s: &str) -> Self {
    match s.to_lowercase().as_str() {
      "get" => HttpMethod::Get,
      "post" => HttpMethod::Post,
      "put" => HttpMethod::Put,
      "delete" => HttpMethod::Delete,
      "patch" => HttpMethod::Patch,
      "head" => HttpMethod::Head,
      "options" => HttpMethod::Options,
      "trace" => HttpMethod::Trace,
      "connect" => HttpMethod::Connect,
      "*" => HttpMethod::Any,
      _ => panic!("Invalid HTTP method"),
    }
  }
}

impl std::fmt::Display for HttpMethod {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      HttpMethod::Get => write!(f, "GET"),
      HttpMethod::Post => write!(f, "POST"),
      HttpMethod::Put => write!(f, "PUT"),
      HttpMethod::Delete => write!(f, "DELETE"),
      HttpMethod::Patch => write!(f, "PATCH"),
      HttpMethod::Head => write!(f, "HEAD"),
      HttpMethod::Options => write!(f, "OPTIONS"),
      HttpMethod::Trace => write!(f, "TRACE"),
      HttpMethod::Connect => write!(f, "CONNECT"),
      HttpMethod::Any => write!(f, "*"),
    }
  }
}
