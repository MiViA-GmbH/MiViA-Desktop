use async_trait::async_trait;
use mivia_domain::ids::BatchId;

use crate::ipc::screens::{AccountView, HoldsView, SeriesReportsView};
use crate::ipc::views::{
    BatchItemsPage, BatchView, FoldersView, HistoryCursor, HistoryPage, NeedsAttentionView,
};
use crate::store::{AccountScope, StoreError};

#[async_trait]
pub trait Queries: Send + Sync {
    async fn list_history(
        &self,
        scope: AccountScope,
        after: Option<HistoryCursor>,
    ) -> Result<HistoryPage, StoreError>;
    async fn list_needs_attention(
        &self,
        scope: AccountScope,
    ) -> Result<NeedsAttentionView, StoreError>;
    async fn get_batch(
        &self,
        scope: AccountScope,
        id: BatchId,
    ) -> Result<Option<BatchView>, StoreError>;
    async fn list_batch_items(
        &self,
        scope: AccountScope,
        id: BatchId,
        after: Option<u32>,
    ) -> Result<BatchItemsPage, StoreError>;
    async fn list_folders(&self, scope: AccountScope) -> Result<FoldersView, StoreError>;
    async fn list_holds(&self, scope: AccountScope) -> Result<HoldsView, StoreError>;
    async fn list_series_reports(
        &self,
        scope: AccountScope,
        id: BatchId,
    ) -> Result<SeriesReportsView, StoreError>;
    async fn get_account(&self, scope: AccountScope) -> Result<Option<AccountView>, StoreError>;
    async fn global_settings(&self) -> Result<Option<String>, StoreError>;
}
