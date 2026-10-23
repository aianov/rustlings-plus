use comment_tree::*;
use std::rc::Rc;

fn sample() -> (Thread, Rc<Comment>, Rc<Comment>, Rc<Comment>) {
    let thread = Thread::new();
    let root = thread.post("ann", "first!");
    let reply = root.reply("bob", "hello ann");
    let deep = reply.reply("cat", "nice thread");
    (thread, root, reply, deep)
}

#[test]
fn replies_build_a_tree_with_paths_and_depth() {
    let (thread, root, reply, deep) = sample();
    assert_eq!(deep.path(), "ann/bob/cat");
    assert_eq!((root.depth(), reply.depth(), deep.depth()), (0, 1, 2));
    assert_eq!(root.children().len(), 1);
    assert_eq!(reply.children()[0].author(), "cat");
    assert_eq!(deep.parent().map(|p| p.author().to_string()), Some("bob".to_string()));
    assert!(root.parent().is_none());
    assert_eq!((root.size(), reply.size(), deep.size()), (3, 2, 1));
    assert_eq!(thread.total(), 3);
    let second = thread.post("dan", "other topic");
    second.reply("eve", "re");
    assert_eq!(thread.total(), 5);
    assert_eq!(thread.roots().len(), 2);
}

#[test]
fn comments_are_edited_through_any_handle() {
    let (thread, root, _reply, _deep) = sample();
    let same = thread.roots().remove(0);
    root.edit("edited");
    assert_eq!(same.text(), "edited", "это один и тот же комментарий, правка видна через любую ссылку");
    assert!(Rc::ptr_eq(&root, &same));
}

#[test]
fn ownership_goes_down_and_never_up() {
    let (_thread, root, reply, deep) = sample();
    assert_eq!(Rc::strong_count(&root), 2, "корень держат тред и наша переменная, дети его не держат");
    assert_eq!(Rc::strong_count(&reply), 2, "ответ держат родитель и наша переменная");
    assert_eq!(Rc::weak_count(&root), 1);
    assert_eq!(Rc::weak_count(&reply), 1);
    assert_eq!(Rc::strong_count(&deep), 2);
}

#[test]
fn removing_a_branch_frees_it_at_once() {
    let (thread, root, reply, deep) = sample();
    root.reply("dan", "sibling");
    assert_eq!(thread.live(), 4);
    drop(deep);
    assert_eq!(thread.live(), 4, "ответ ещё в дереве");
    assert!(thread.remove(&reply), "ветвь была в дереве");
    assert!(!thread.remove(&reply), "второй раз удалять нечего");
    drop(reply);
    assert_eq!(thread.live(), 2, "ответ и его потомок освобождены сразу, остались корень и сосед");
    assert_eq!(thread.total(), 2);
    assert_eq!(root.children().len(), 1);
}

#[test]
fn a_removed_branch_lives_while_someone_holds_it() {
    let (thread, _root, reply, deep) = sample();
    assert!(thread.remove(&reply));
    assert!(reply.parent().is_none(), "снятая ветвь больше не знает родителя");
    assert_eq!(deep.path(), "bob/cat", "снятая ветвь стала самостоятельным деревом");
    assert_eq!(thread.live(), 3, "пока на ветвь есть ссылки снаружи, она жива");
    drop(reply);
    assert_eq!(thread.live(), 2, "ответ освободился вместе с последней ссылкой, а потомка держит наша переменная deep");
    assert!(deep.parent().is_none(), "родитель умер, слабая ссылка вверх не оживляет его");
    assert_eq!(deep.path(), "cat");
}

#[test]
fn removing_a_root_works_too() {
    let (thread, root, _reply, _deep) = sample();
    let other = thread.post("zed", "x");
    assert!(thread.remove(&root));
    assert_eq!(thread.roots().len(), 1);
    assert!(Rc::ptr_eq(&thread.roots()[0], &other));
}

#[test]
fn nothing_leaks_when_everything_is_dropped() {
    let counter = {
        let (thread, root, reply, deep) = sample();
        reply.reply("dan", "x");
        root.reply("eve", "y");
        let counter = thread.live_counter();
        assert_eq!(counter.get(), 5);
        drop((root, reply, deep));
        assert_eq!(counter.get(), 5, "дерево держит тред");
        counter
    };
    assert_eq!(counter.get(), 0, "после уничтожения треда живых комментариев не осталось: если не ноль, где-то цикл из Rc");
}

#[test]
fn tags_come_from_a_lazily_built_dictionary() {
    let thread = Thread::new();
    let c = thread.post("ann", "love #rust, and #Rust! #unknown #cats #rust");
    assert_eq!(c.tags(), vec!["programming", "pets"], "известные теги по порядку появления, без повторов, регистр не важен");
    assert!(thread.post("bob", "no # tags here #").tags().is_empty());
    assert_eq!(thread.post("cat", "#food").tags(), vec!["life"]);
    assert_eq!(dictionary_builds(), 1, "словарь тегов строится один раз за всё время работы");
}
