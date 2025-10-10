fn main() {
	let git2 = vergen_git2::Git2Builder::default()
		.commit_count(true)
		.sha(true)
		.build()
		.unwrap();
	vergen_git2::Emitter::default()
		.add_instructions(&git2)
		.unwrap()
		.emit()
		.unwrap();
}
