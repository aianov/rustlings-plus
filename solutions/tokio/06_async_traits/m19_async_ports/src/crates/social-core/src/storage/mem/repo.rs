use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::chat::Message;
use crate::comments::Comment;
use crate::domain::{
    CommentId, Cursor, DomainError, Post, PostId, Timestamp, User, UserId, Visibility,
};
use crate::network::Network;
use crate::ports::{
    AuthorStats, BoxFuture, ChatRepo, FollowCounts, Liked, PostItem, PostPage, PostRepo, PostStore,
    Published, Repos, UserRepo, UserStore,
};
use crate::trends::Trends;

pub struct MemRepo<U, P> {
    network: Mutex<Network<U, P>>,
}

impl<U, P> MemRepo<U, P>
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    pub fn new(network: Network<U, P>) -> MemRepo<U, P> {
        MemRepo {
            network: Mutex::new(network),
        }
    }

    pub fn repos(network: Network<U, P>) -> Repos {
        let repo = Arc::new(MemRepo::new(network));
        Repos {
            users: repo.clone(),
            posts: repo.clone(),
            chats: repo,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Network<U, P>> {
        self.network.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn item<U: UserStore, P: PostStore>(network: &Network<U, P>, post: &Post) -> PostItem {
    PostItem::of(post, network.user(post.author()))
}

impl<U, P> UserRepo for MemRepo<U, P>
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    fn register(&self, name: String) -> BoxFuture<'_, Result<UserId, DomainError>> {
        Box::pin(async move { self.lock().register(&name) })
    }

    fn user(&self, id: UserId) -> BoxFuture<'_, Option<User>> {
        Box::pin(async move { self.lock().user(id).cloned() })
    }

    fn find(&self, name: String) -> BoxFuture<'_, Option<User>> {
        Box::pin(async move { self.lock().find_user(&name).cloned() })
    }

    fn follow(&self, who: UserId, whom: UserId) -> BoxFuture<'_, Result<bool, DomainError>> {
        Box::pin(async move { self.lock().follow(who, whom) })
    }

    fn mute(&self, who: UserId, whom: UserId) -> BoxFuture<'_, Result<bool, DomainError>> {
        Box::pin(async move { self.lock().mute(who, whom) })
    }

    fn is_following(&self, who: UserId, whom: UserId) -> BoxFuture<'_, bool> {
        Box::pin(async move { self.lock().is_following(who, whom) })
    }

    fn follow_counts(&self, id: UserId) -> BoxFuture<'_, FollowCounts> {
        Box::pin(async move {
            let network = self.lock();
            FollowCounts {
                followers: network.followers_count(id),
                following: network.following_count(id),
            }
        })
    }

    fn count(&self) -> BoxFuture<'_, usize> {
        Box::pin(async move { self.lock().user_count() })
    }
}

impl<U, P> PostRepo for MemRepo<U, P>
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    fn publish(
        &self,
        author: UserId,
        text: String,
        visibility: Visibility,
    ) -> BoxFuture<'_, Result<Published, DomainError>> {
        Box::pin(async move {
            let mut network = self.lock();
            let post = network.publish(author, &text, visibility)?;
            let audience = network.audience(post);
            Ok(Published { post, audience })
        })
    }

    fn like(&self, user: UserId, post: PostId) -> BoxFuture<'_, Result<Liked, DomainError>> {
        Box::pin(async move {
            let mut network = self.lock();
            let changed = network.like(user, post)?;
            let author = network
                .post(post)
                .map(Post::author)
                .ok_or(DomainError::UnknownPost(post))?;
            Ok(Liked { changed, author })
        })
    }

    fn delete(&self, who: UserId, post: PostId) -> BoxFuture<'_, Result<bool, DomainError>> {
        Box::pin(async move { self.lock().delete_post(who, post) })
    }

    fn post(&self, id: PostId) -> BoxFuture<'_, Option<PostItem>> {
        Box::pin(async move {
            let network = self.lock();
            network.post(id).map(|post| item(&network, post))
        })
    }

    fn feed(&self, viewer: UserId, skip: usize, take: usize) -> BoxFuture<'_, Vec<PostItem>> {
        Box::pin(async move {
            let network = self.lock();
            let mut items = Vec::with_capacity(take.min(network.post_count()));
            let page = network.feed(viewer).skip(skip).take(take);
            items.extend(page.map(|post| item(&network, post)));
            items
        })
    }

    fn by_author(
        &self,
        viewer: UserId,
        author: UserId,
        after: Option<Cursor>,
        size: usize,
    ) -> BoxFuture<'_, Result<PostPage, DomainError>> {
        Box::pin(async move {
            let network = self.lock();
            let page = network.author_page(viewer, author, after, size)?;
            Ok(PostPage {
                posts: page.posts.iter().map(|post| item(&network, post)).collect(),
                next: page.next,
            })
        })
    }

    fn author_stats(&self, author: UserId) -> BoxFuture<'_, AuthorStats> {
        Box::pin(async move {
            let network = self.lock();
            AuthorStats {
                posts: network.post_count_of(author),
                likes: network.likes_received(author),
            }
        })
    }

    fn comment(
        &self,
        author: UserId,
        post: PostId,
        text: String,
    ) -> BoxFuture<'_, Result<CommentId, DomainError>> {
        Box::pin(async move { self.lock().comment(author, post, &text) })
    }

    fn reply(
        &self,
        author: UserId,
        parent: CommentId,
        text: String,
    ) -> BoxFuture<'_, Result<CommentId, DomainError>> {
        Box::pin(async move { self.lock().reply(author, parent, &text) })
    }

    fn replies(
        &self,
        viewer: UserId,
        post: PostId,
        parent: Option<CommentId>,
    ) -> BoxFuture<'_, Result<Vec<Comment>, DomainError>> {
        Box::pin(async move {
            let network = self.lock();
            let replies = network.replies(viewer, post, parent)?;
            Ok(replies.into_iter().cloned().collect())
        })
    }

    fn trend_snapshot(&self) -> BoxFuture<'_, (Trends, Timestamp)> {
        Box::pin(async move {
            let network = self.lock();
            (network.trend_likes(), network.now())
        })
    }

    fn count(&self) -> BoxFuture<'_, usize> {
        Box::pin(async move { self.lock().post_count() })
    }
}

impl<U, P> ChatRepo for MemRepo<U, P>
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    fn send(
        &self,
        from: UserId,
        to: UserId,
        text: String,
    ) -> BoxFuture<'_, Result<Message, DomainError>> {
        Box::pin(async move { self.lock().send_message(from, to, &text).cloned() })
    }

    fn history(
        &self,
        user: UserId,
        peer: UserId,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<Message>, DomainError>> {
        Box::pin(async move {
            self.lock()
                .history(user, peer, limit)
                .map(<[Message]>::to_vec)
        })
    }
}
