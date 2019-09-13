//! Traits defining shared behavior for interacting with notifications in the Thorium client
use uuid::Uuid;

use super::GenericClient;
use crate::client::Error;
use crate::models::backends::NotificationSupport;
use crate::models::{KeySupport, Notification, NotificationParams, NotificationRequest};
use crate::{add_query, send, send_build};

/// Describes client that can interact with notifications related to a
/// specific entity type in the Thorium API
pub trait NotificationsClient: GenericClient {
    /// The underlying type that the notifications are related to (see [`NotificationSupport`])
    type NotificationSupport: NotificationSupport;

    /// Creates a [`Notification`] in Thorium for the [`NotificationSupport`] entity
    ///
    /// # Arguments
    ///
    /// * `req` - The notification create request
    /// * `params` - The notification create params
    async fn create_notification_generic<K>(
        &self,
        key: K,
        req: &NotificationRequest<Self::NotificationSupport>,
        params: &NotificationParams,
    ) -> Result<reqwest::Response, Error>
    where
        K: AsRef<<Self::NotificationSupport as KeySupport>::Key>,
    {
        // build url for creating a notification for an image
        let url = format!(
            "{base}/notifications/{key}",
            base = self.base_url(),
            key = Self::NotificationSupport::key_url(key.as_ref(), None)
        );
        // build our query params
        let mut query = vec![];
        add_query!(query, "expire", params.expire);
        // build request
        let req = self
            .client()
            .post(&url)
            .json(req)
            .header("authorization", self.token())
            .query(&query);
        // send this request
        send!(self.client(), req)
    }

    /// Gets all notifications for the underlying [`NotificationSupport`] entity
    ///
    /// # Arguments
    ///
    /// * `key` - The key to use to access the `NotificationSupport` entity
    async fn get_notifications_generic<K>(
        &self,
        key: K,
    ) -> Result<Vec<Notification<Self::NotificationSupport>>, Error>
    where
        K: AsRef<<Self::NotificationSupport as KeySupport>::Key>,
    {
        // build url for getting image notifications
        let url = format!(
            "{base}/notifications/{key}",
            base = self.base_url(),
            key = Self::NotificationSupport::key_url(key.as_ref(), None)
        );
        // build request
        let req = self
            .client()
            .get(&url)
            .header("authorization", self.token());
        // send this request
        send_build!(
            self.client(),
            req,
            Vec<Notification<Self::NotificationSupport>>
        )
    }

    /// Delete a notification from the [`NotificationSupport`] entity
    ///
    /// # Arguments
    ///
    /// * `key` - The key to use to access the `NotificationSupport` entity
    /// * `id` - The unique id of the notification
    async fn delete_notification_generic<K>(
        &self,
        key: K,
        id: &Uuid,
    ) -> Result<reqwest::Response, Error>
    where
        K: AsRef<<Self::NotificationSupport as KeySupport>::Key>,
    {
        // build url for deleting an image log
        let url = format!(
            "{base}/notifications/{key}/{id}",
            base = self.base_url(),
            key = Self::NotificationSupport::key_url(key.as_ref(), None),
            id = id
        );
        // build request
        let req = self
            .client()
            .delete(&url)
            .header("authorization", self.token());
        // send this request
        send!(self.client(), req)
    }
}
