#![feature(pattern)]

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::str::pattern::{Pattern, Searcher, SearchStep};

enum JSJunkState<'a> {
    Normal,
    InLiteral(&'a str),
}

#[derive(Clone,Copy)]
struct JSJunkSearcher<'a> {
    haystack: &'a str,
    pos: usize,
}

unsafe impl<'a> Searcher<'a> for JSJunkSearcher<'a> {
    fn haystack(&self) -> &'a str {
        self.haystack
    }

    fn next(&mut self) -> SearchStep {
        eprintln!("JSJunk: next() at position {}", self.pos);
        if self.pos >= self.haystack.len() {
            eprintln!("JSJunk: Done!");
            return SearchStep::Done;
        }
        
        let start = self.pos;
        let post = self.haystack.split_at(self.pos).1;

        if post.starts_with("\n") {
            self.pos += 1;
            eprintln!("JSJunk: Newline [[{}]]", &self.haystack[start .. self.pos - 1]);
            SearchStep::Match(start, self.pos)
        } else if post.starts_with("//") {
            while self.pos < self.haystack.len() &&
                ! self.haystack.split_at(self.pos).1.starts_with("\n") {
                    self.pos += 1;
                }
            eprintln!("JSJunk: Comment [[{}]]", &self.haystack[start .. self.pos - 1]);
            SearchStep::Match(start, self.pos)
        } else if post.starts_with("/*") {
            while self.pos < self.haystack.len() {
                let newpost = self.haystack.split_at(self.pos).1;
                if newpost.starts_with("*/") { break; }
                self.pos += 1;
            }
            eprintln!("JSJunk: Block comment [[{}]]", &self.haystack[start .. self.pos - 1]);
            SearchStep::Match(start, self.pos)
        } else { // No match, skip
            let mut state = JSJunkState::Normal;
            while self.pos < self.haystack.len() {
                let newpost = self.haystack.split_at(self.pos).1;
                match state {
                    JSJunkState::Normal => {
                        if newpost.starts_with("'") || newpost.starts_with("\"") {
                            state = JSJunkState::InLiteral(&self.haystack[self.pos..self.pos]);
                        } else if newpost.starts_with("//")
                            || newpost.starts_with("/*")
                            || newpost.starts_with("\n") {
                                break;
                        }
                    },
                    JSJunkState::InLiteral(c) => {
                        if newpost.starts_with(c) {
                            state = JSJunkState::Normal;
                        }
                    }
                }
                self.pos += 1;
            }
            eprintln!("JSJunk: Rejecting [[{}]]", &self.haystack[start .. self.pos - 1]);
            SearchStep::Reject(start, self.pos)
        }
    }
}

struct JSJunkPattern {}

impl<'a> Pattern<'a> for JSJunkPattern {
    type Searcher = JSJunkSearcher<'a>;

    fn into_searcher(self, haystack: &'a str) -> Self::Searcher {
        JSJunkSearcher {
            haystack: haystack,
            pos: 0,
        }
    }
}

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("helper.js");
    let mut f = File::create(&dest_path).unwrap();

    //let helperjs_src = include_str!("src/helper.js").replace("\n", " ");
    let helperjs_src = include_str!("src/helper.js").replace(JSJunkPattern {}, " ");

    f.write_all(helperjs_src.as_bytes()).unwrap();
}