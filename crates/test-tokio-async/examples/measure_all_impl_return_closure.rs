struct Worktree(String);

#[hotpath::measure_all]
impl Worktree {
    fn label<'a>(&'a self) -> impl Fn() -> &'a str + 'a {
        || self.0.as_str()
    }
}

fn main() {
    let worktree = Worktree("local-worktree".to_string());
    let _ = worktree.label()();
}
