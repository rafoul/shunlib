pub trait SqlTemplate {
    fn name(&self) -> &str;

    fn sql(&self) -> &str;
}

impl SqlTemplate for (&str, &str) {
    fn name(&self) -> &str {
        self.0
    }

    fn sql(&self) -> &str {
        self.1
    }
}
