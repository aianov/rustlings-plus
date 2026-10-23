use contracts::Role;
use grpc_kit::Stamp;
use jobs::{Failure, Job, Queue};
use proto::social::v1::posts_client::PostsClient;
use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{AuthorPostsRequest, OwnProfileRequest, PostAction};
use serde_json::json;
use tonic::transport::Channel;
use tonic::{Code, Status};

use crate::repository::BoxFuture;
use crate::service::{DELETE_KIND, DeletionQueue, PostEraser, ProfileKeeper};

fn failure(service: &str, status: &Status) -> Failure {
    let reason = format!("{service}: {}", status.message());
    match status.code() {
        Code::Unavailable
        | Code::DeadlineExceeded
        | Code::Cancelled
        | Code::Aborted
        | Code::ResourceExhausted
        | Code::Unknown
        | Code::Internal => Failure::Transient(reason),
        _ => Failure::Permanent(reason),
    }
}

fn on_behalf(user: u64) -> Stamp {
    Stamp::fresh().user(user).role(Role::User.as_str())
}

pub struct SagaProfiles {
    users: Channel,
}

impl SagaProfiles {
    pub fn new(users: Channel) -> SagaProfiles {
        SagaProfiles { users }
    }

    fn client(
        &self,
        user: u64,
    ) -> UsersClient<tonic::service::interceptor::InterceptedService<Channel, Stamp>> {
        UsersClient::with_interceptor(self.users.clone(), on_behalf(user))
    }
}

impl ProfileKeeper for SagaProfiles {
    fn hide(&self, user_id: u64) -> BoxFuture<'_, Result<(), Failure>> {
        Box::pin(async move {
            let done = self
                .client(user_id)
                .hide_profile(OwnProfileRequest {})
                .await;
            done.map(drop)
                .map_err(|status| failure("user-service", &status))
        })
    }

    fn show(&self, user_id: u64) -> BoxFuture<'_, Result<(), Failure>> {
        Box::pin(async move {
            let done = self
                .client(user_id)
                .show_profile(OwnProfileRequest {})
                .await;
            done.map(drop)
                .map_err(|status| failure("user-service", &status))
        })
    }

    fn erase(&self, user_id: u64) -> BoxFuture<'_, Result<(), Failure>> {
        Box::pin(async move {
            let done = self
                .client(user_id)
                .delete_profile(OwnProfileRequest {})
                .await;
            done.map(drop)
                .map_err(|status| failure("user-service", &status))
        })
    }
}

pub struct SagaPosts {
    posts: Channel,
}

impl SagaPosts {
    pub fn new(posts: Channel) -> SagaPosts {
        SagaPosts { posts }
    }

    fn client(
        &self,
        user: u64,
    ) -> PostsClient<tonic::service::interceptor::InterceptedService<Channel, Stamp>> {
        PostsClient::with_interceptor(self.posts.clone(), on_behalf(user))
    }
}

impl PostEraser for SagaPosts {
    fn page(&self, user_id: u64, limit: u32) -> BoxFuture<'_, Result<Vec<u64>, Failure>> {
        Box::pin(async move {
            let ask = AuthorPostsRequest {
                author_id: user_id,
                limit,
            };
            let reply = self.client(user_id).author_posts(ask).await;
            let reply = reply.map_err(|status| failure("post-service", &status))?;
            Ok(reply
                .into_inner()
                .posts
                .into_iter()
                .map(|post| post.id)
                .collect())
        })
    }

    fn delete(&self, user_id: u64, post_id: u64) -> BoxFuture<'_, Result<(), Failure>> {
        Box::pin(async move {
            match self
                .client(user_id)
                .delete_post(PostAction { post_id })
                .await
            {
                Ok(..) => Ok(()),
                Err(status) if status.code() == Code::NotFound => Ok(()),
                Err(status) => Err(failure("post-service", &status)),
            }
        })
    }
}

pub struct JobDeletions {
    queue: Queue,
}

impl JobDeletions {
    pub fn new(queue: Queue) -> JobDeletions {
        JobDeletions { queue }
    }
}

impl DeletionQueue for JobDeletions {
    fn request(&self, saga: String, user_id: u64) -> BoxFuture<'_, Result<(), Failure>> {
        Box::pin(async move {
            let job = Job::new(&saga, DELETE_KIND, json!({ "user_id": user_id }));
            self.queue
                .enqueue(&job)
                .await
                .map(drop)
                .map_err(|error| Failure::Transient(error.to_string()))
        })
    }
}
