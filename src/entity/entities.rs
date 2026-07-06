use crate::entity::{Entity, EntityFlags, EntityInfo};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct DummyObj {}

#[derive(Debug, Clone)]
pub struct Dummy {
    pool: sqlx::PgPool,
}

impl Dummy {
    /// Creates a new instance of the Dummy entity.
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

impl Entity for Dummy {
    type FullObject = DummyObj;
    type PublicObject = DummyObj;
    type SummaryObject = DummyObj;
    type CreateObject = DummyObj;

    fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }

    fn name(&self) -> &'static str {
        "Dummy"
    }

    fn target_type(&self) -> &'static str {
        "dummy"
    }

    fn cdn_folder(&self) -> &'static str {
        "dummys"
    }

    async fn flags(&self, _id: &str) -> Result<EntityFlags, crate::Error> {
        Ok(EntityFlags::empty())
    }

    async fn get_info(&self, _id: &str) -> Result<Option<EntityInfo>, crate::Error> {
        Ok(None)
    }

    async fn get_full(&self, _id: &str) -> Result<Self::FullObject, crate::Error> {
        Ok(DummyObj {})
    }

    async fn get_public(&self, _id: &str) -> Result<Self::PublicObject, crate::Error> {
        Ok(DummyObj {})
    }

    async fn get_summary(&self, _id: &str) -> Result<Self::SummaryObject, crate::Error> {
        Ok(DummyObj {})
    }

    async fn create(&self, _obj: Self::CreateObject) -> Result<String, crate::Error> {
        Ok("dummy_id".to_string())
    }
}