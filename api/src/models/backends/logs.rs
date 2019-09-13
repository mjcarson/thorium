//! Wrappers for interacting with status logs within Thorium with different backends
//! Currently only Redis is supported

use chrono::Utc;
use std::collections::HashMap;

use crate::models::{
    Actions, JobActions, RawJob, Reaction, ReactionActions, StatusRequest, StatusUpdate,
};
use crate::utils::ApiError;
use crate::{deserialize, force_serialize};

impl StatusRequest {
    /// Builds the update part of the status update based on a RawJob and an action
    ///
    /// # Arguments
    ///
    /// * `job` - The job this status update is for
    /// * `action` - The action that caused this status update
    pub(self) fn build_update_job(job: &RawJob, action: &JobActions) -> HashMap<String, String> {
        let mut update = HashMap::new();
        // inject job id
        update.insert("job".to_string(), job.id.to_string());

        // build correct update based on action
        match action {
            JobActions::Created => {
                update.insert("reaction".to_owned(), job.reaction.to_string());
                update.insert("id".to_owned(), job.id.to_string());
                update.insert("group".to_owned(), job.group.to_owned());
                update.insert("pipeline".to_owned(), job.pipeline.to_owned());
                update.insert("stage".to_owned(), job.stage.to_owned());
                update.insert("deadline".to_owned(), job.deadline.to_string());
                update.insert("status".to_owned(), force_serialize!(&job.status));
            }
            JobActions::Running => {
                update.insert("status".to_string(), "Running".to_string());
            }
            JobActions::Reset(_) => {
                update.insert("status".to_string(), "Created".to_string());
            }
            JobActions::Completed => {
                update.insert("status".to_string(), "Completed".to_string());
            }
            JobActions::Errored => {
                update.insert("status".to_string(), "Failed".to_string());
            }
        };
        // return update
        update
    }

    /// Build the update part of the status update based on a Reaction and an action
    ///
    /// # Arguments
    ///
    /// * `reaction` - The reaction this status update is for
    /// * `action` - The action that caused this status update to occur
    pub(self) fn build_update_reaction(
        reaction: &Reaction,
        action: &ReactionActions,
    ) -> HashMap<String, String> {
        let mut update = HashMap::new();

        match action {
            ReactionActions::Created => {
                update.insert("id".to_owned(), reaction.id.to_string());
                update.insert("group".to_owned(), reaction.group.to_owned());
                update.insert("pipeline".to_owned(), reaction.pipeline.to_owned());
                update.insert("status".to_owned(), force_serialize!(&reaction.status));
                update.insert(
                    "current_stage".to_owned(),
                    reaction.current_stage.to_string(),
                );
                update.insert("sla".to_owned(), reaction.sla.to_string());
            }
            ReactionActions::Proceeding => {
                update.insert(
                    "current_stage".to_owned(),
                    reaction.current_stage.to_string(),
                );
            }
            ReactionActions::Completed => {
                update.insert("status".to_owned(), "Completed".to_owned());
            }
            ReactionActions::Failed => {
                update.insert("status".to_owned(), "Failed".to_owned());
            }
            ReactionActions::Enabled => {
                update.insert("status".to_owned(), "Enabled".to_owned());
            }
            ReactionActions::Disabled => {
                update.insert("status".to_owned(), "Disabled".to_owned());
            }
        };
        update
    }

    /// Build a job claim status update based on a RawJob and a worker
    ///
    /// # Arguments
    ///
    /// * `job` - The job that this update is for
    /// * `worker` - The worker that claimed this job
    pub fn claim_job<T: Into<String>>(job: &RawJob, worker: T) -> Self {
        // build status update
        let mut update = HashMap::with_capacity(2);
        update.insert("status".to_string(), "running".to_string());
        update.insert("worker".to_string(), worker.into());
        // build a status request from this job claim
        StatusRequest {
            group: job.group.clone(),
            pipeline: job.pipeline.clone(),
            reaction: job.reaction.to_string(),
            action: Actions::JobRunning,
            update,
        }
    }

    /// Build a status update based on a RawJob and an action
    ///
    /// # Arguments
    ///
    /// * `job` - The job that this update is for
    /// * `action` - The action that caused this update
    pub fn from_job(job: &RawJob, action: JobActions) -> Self {
        // build the update for this action
        let update = Self::build_update_job(job, &action);
        // convert this action
        let action_cast = match action {
            JobActions::Created => Actions::JobCreated,
            JobActions::Running => Actions::JobRunning,
            JobActions::Reset(requestor) => Actions::JobReset(requestor),
            JobActions::Completed => Actions::JobCompleted,
            JobActions::Errored => Actions::JobFailed,
        };
        // build our status request
        StatusRequest {
            group: job.group.clone(),
            pipeline: job.pipeline.clone(),
            reaction: job.reaction.to_string(),
            action: action_cast,
            update,
        }
    }

    /// Build a status update based on a Reaction and an action
    ///
    /// # Arguments
    ///
    /// * `reaction` - The reaction that this update is for
    /// * `action` - The action that caused this update
    pub fn from_reaction(reaction: &Reaction, action: ReactionActions) -> Self {
        let action_cast = match action {
            ReactionActions::Created => Actions::ReactionCreated,
            ReactionActions::Completed => Actions::ReactionCompleted,
            ReactionActions::Proceeding => Actions::ReactionProceeding,
            ReactionActions::Failed => Actions::ReactionFailed,
            ReactionActions::Enabled => Actions::ReactionEnabled,
            ReactionActions::Disabled => Actions::ReactionDisabled,
        };

        StatusRequest {
            group: reaction.group.clone(),
            pipeline: reaction.pipeline.clone(),
            reaction: reaction.id.to_string(),
            action: action_cast,
            update: Self::build_update_reaction(reaction, &action),
        }
    }
}

impl StatusUpdate {
    /// Creates a status update object from a job
    ///
    /// # Arguments
    ///
    /// * `request` - The status update request to insert into the status log
    /// * `msg` - The message that we should include in this status update
    pub(super) fn new(request: StatusRequest, msg: Option<String>) -> Self {
        // create status update
        StatusUpdate {
            group: request.group.clone(),
            pipeline: request.pipeline.clone(),
            reaction: request.reaction.to_string(),
            timestamp: Utc::now(),
            action: request.action,
            msg,
            update: request.update,
        }
    }

    /// Okay wrap deserializing StatusUpdate since Error can't be deserialized
    ///
    /// # Arguments
    ///
    /// * `raw` - The string to deserialize into a status update
    pub fn deserialize(raw: &str) -> Result<Self, ApiError> {
        Ok(deserialize!(raw))
    }
}
