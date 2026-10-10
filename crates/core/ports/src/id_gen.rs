use uuid::Uuid;

pub trait IdGen: Send + Sync {
    fn uuid(&self) -> Uuid;
}
