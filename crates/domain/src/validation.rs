//! Problems with input values, found by domain rules before anything is stored. The server turns
//! them into `invalid_request` field errors (architecture.md §10), so a form shows every problem
//! at once.

/// Why a value was rejected. The API sends [`Problem::code`]; the UI translates it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// Empty or missing.
    Required,
    /// Not in the expected form.
    Invalid,
    TooShort,
    TooLong,
    OutOfRange,
    /// Another record already uses the value (a username, an email).
    Taken,
    /// Does not match what is stored (a current password, a setup code).
    Incorrect,
}

impl Problem {
    pub fn code(self) -> &'static str {
        match self {
            Problem::Required => "required",
            Problem::Invalid => "invalid",
            Problem::TooShort => "too_short",
            Problem::TooLong => "too_long",
            Problem::OutOfRange => "out_of_range",
            Problem::Taken => "taken",
            Problem::Incorrect => "incorrect",
        }
    }
}

/// Why one value was rejected, with an English explanation for developers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    pub problem: Problem,
    pub message: String,
}

impl Rejection {
    pub fn new(problem: Problem, message: impl Into<String>) -> Self {
        Self {
            problem,
            message: message.into(),
        }
    }
}

/// A rejected value and where it was in the input, such as `admin.email`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldProblem {
    pub path: String,
    pub problem: Problem,
    pub message: String,
}

/// Collects the problems of several fields.
#[derive(Debug, Default)]
pub struct Problems(Vec<FieldProblem>);

impl Problems {
    pub fn new() -> Self {
        Self::default()
    }

    /// Keeps the value, or records the rejection under `path` and returns `None`.
    pub fn check<T>(&mut self, path: &str, result: Result<T, Rejection>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(rejection) => {
                self.add(path, rejection);
                None
            }
        }
    }

    pub fn add(&mut self, path: &str, rejection: Rejection) {
        self.0.push(FieldProblem {
            path: path.to_owned(),
            problem: rejection.problem,
            message: rejection.message,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `Ok` when nothing was rejected.
    pub fn finish(self) -> Result<(), Vec<FieldProblem>> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(self.0)
        }
    }

    pub fn into_vec(self) -> Vec<FieldProblem> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_every_rejection() {
        let mut problems = Problems::new();
        let kept = problems.check("a", Ok::<_, Rejection>(1));
        let lost = problems.check::<i32>("b", Err(Rejection::new(Problem::TooLong, "long")));
        problems.add("c", Rejection::new(Problem::Taken, "taken"));
        assert_eq!(kept, Some(1));
        assert_eq!(lost, None);
        let all = problems.finish().unwrap_err();
        assert_eq!(
            all.iter()
                .map(|p| (p.path.as_str(), p.problem.code()))
                .collect::<Vec<_>>(),
            [("b", "too_long"), ("c", "taken")]
        );
    }
}
