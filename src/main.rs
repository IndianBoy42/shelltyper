use itertools::{izip, Itertools};
use std::borrow::Cow;
use std::io::stdout;
use std::ops::Range;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use tui::layout::{Constraint, Layout, Rect};
use tui::style::{Color, Style};
use tui::text::{Span, Spans};
use tui::widgets::{self, Block, BorderType, Borders, Paragraph, Widget};
use tui::Frame;
use tui::{backend::CrosstermBackend, Terminal};

type Backend = CrosstermBackend<std::io::Stdout>;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Event {
    Tick,
    Input(KeyEvent),
}

const HELP: &str = "\
monkeytype in the shell

Usage: shelltyper

No arguments yet
";
#[derive(Debug)]
struct Args {}
impl Default for Args {
    fn default() -> Self {
        Args {}
    }
}
impl Args {
    fn parse_env() -> Args {
        let mut pargs = pico_args::Arguments::from_env();

        if pargs.contains(["-h", "--help"]) {
            print!("{}", HELP);
            std::process::exit(0);
        }

        let dargs = Self::default();

        Args { ..dargs }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Hello, world!");

    let args = Args::parse_env();

    enable_raw_mode()?;

    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend)?;

    let rx = input_handling_thread(&terminal);

    let mut app = App::new();

    terminal.clear()?;

    loop {
        // Draw everything:
        terminal.draw(|f| app.draw(f))?;

        match rx.recv()? {
            Event::Tick => app.on_tick()?,
            Event::Input(key) => match key.code {
                KeyCode::Char('q') => {
                    disable_raw_mode()?;
                    execute!(
                        terminal.backend_mut(),
                        LeaveAlternateScreen,
                        DisableMouseCapture
                    )?;
                    terminal.show_cursor()?;
                    break;
                }
                code => app.on_key(code)?,
            },
        };
    }

    todo!("Finish this")
}

fn input_handling_thread(_terminal: &Terminal<Backend>) -> Receiver<Event> {
    let (tx, rx) = mpsc::channel();

    let tick_rate = Duration::from_millis(10);
    thread::spawn(move || {
        let mut last_tick = Instant::now();
        loop {
            // Poll for tick rate duration, if no events, sent tick event.
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            // Poll for events
            if event::poll(timeout).unwrap() {
                if let CEvent::Key(key) = event::read().unwrap() {
                    tx.send(Event::Input(key)).unwrap();
                }
            }

            // Send tick event regularly
            if last_tick.elapsed() >= tick_rate {
                tx.send(Event::Tick).unwrap();
                last_tick = Instant::now();
            }
        }
    });

    rx
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum TargetStringType {
    Infinite,
    Words(usize),
}
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum TestState {
    Pre,
    Running,
    Post,
}

#[derive(Debug)]
struct App {
    target_type: TargetStringType,
    target_str: String,
    enterd_str: String,
    target_words: Vec<usize>,
    enterd_words: Vec<usize>,
    running: TestState,
    start: Instant,
    now: Instant,
}
impl App {
    fn new() -> App {
        App {
            target_type: TargetStringType::Infinite,
            running: TestState::Pre,
            target_str: String::new(),
            enterd_str: String::new(),
            target_words: Vec::new(),
            enterd_words: Vec::new(),
            start: Instant::now(),
            now: Instant::now(),
        }
    }

    fn new_target_string(&mut self, ty: TargetStringType) {
        match ty {
            TargetStringType::Infinite => {
                self.target_str = "an example infinite string ...".to_owned();
            }
            TargetStringType::Words(_) => {
                self.target_str = "an example finite string".to_owned();
            }
        }
        self.enterd_str = String::with_capacity(self.target_str.len());

        self.target_words = self
            .target_str
            .char_indices()
            .filter(|&(i, c)| c == ' ')
            .map(|(i, c)| i)
            .collect();
        self.enterd_words = Vec::with_capacity(self.target_words.len());
        self.enterd_words.push(0);
    }

    fn target_is_infinite(&self) -> bool {
        matches!(self.target_type, TargetStringType::Infinite)
    }

    fn on_tick(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.running == TestState::Running {
            // TODO: update stats
            self.now = Instant::now();
        }
        Ok(())
    }

    fn on_key(&mut self, key: KeyCode) -> Result<(), Box<dyn std::error::Error>> {
        let ok = match key {
            KeyCode::Char(' ') | KeyCode::Right | KeyCode::Enter | KeyCode::Tab => {
                self.enterd_str.push(' ');
                *self.enterd_words.last_mut().unwrap() += 1;
                if self.enterd_words.len() == self.target_words.len() {
                    self.running = TestState::Post;
                }
                self.enterd_words.push(0);
            }
            KeyCode::Char(c) => {
                if self.running != TestState::Post {
                    self.enterd_str.push(c);
                    *self.enterd_words.last_mut().unwrap() += 1;
                    if self.running == TestState::Pre {
                        self.running = TestState::Running;
                        self.start = Instant::now();
                    }
                }
            }
            KeyCode::Backspace => {
                match self.enterd_str.pop() {
                    Some(' ') => {
                        self.enterd_str.push(' ');
                    }
                    Some(c) => {
                        *self.enterd_words.last_mut().unwrap() += 1;
                    }
                    None => {
                        // TODO: reset timer? nah
                    }
                }
            }
            KeyCode::Esc => {
                // Reset
                self.running = TestState::Pre;
                self.now = Instant::now();
                self.new_target_string(TargetStringType::Infinite)
            }
            // TODO: any more functions needed?
            KeyCode::Left => {}
            KeyCode::Up => {}
            KeyCode::Down => {}
            KeyCode::Home => {}
            KeyCode::End => {}
            KeyCode::PageUp => {}
            KeyCode::PageDown => {}
            KeyCode::BackTab => {}
            KeyCode::Delete => {}
            KeyCode::Insert => {}
            KeyCode::F(_) => {}
            KeyCode::Null => {}
        };

        Ok(ok)
    }

    fn draw(&self, f: &mut Frame<Backend>) {
        let chunks = Layout::default()
            .direction(tui::layout::Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(3 + 5),
                Constraint::Max(3 + 5),
            ])
            .split(f.size());

        self.title_widget(f, chunks[0]);
        self.text_widget(f, chunks[1]);
        self.stats_widget(f, chunks[2]);
    }

    fn title_widget(&self, f: &mut Frame<Backend>, size: Rect) {
        let par = Paragraph::new(vec![Spans::from(vec![
            Span::raw("Hello World\n"),
            Span::styled(
                "This is stylish",
                Style::default().fg(Color::Green).bg(Color::Red),
            ),
        ])]);
        let block = Block::default()
            .title("Title")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White))
            .border_type(BorderType::Rounded)
            .style(Style::default().bg(Color::Black));

        f.render_widget(par.block(block), size)
    }
    fn text_widget(&self, f: &mut Frame<Backend>, size: Rect) {
        const STRINGS_CLEARED_BEFORE_FINISH: &str =
            "BUG: Clear the target, user strings when they are complete before drawing";
        let block = Block::default()
            .title("Text")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White))
            .border_type(BorderType::Rounded)
            .style(Style::default().bg(Color::Black));

        let inner = block.inner(size);
        let width = inner.width;

        let completed_word_style = Style::default().bg(Color::Black).fg(Color::DarkGray);
        let completed_part_style = Style::default().bg(Color::Black).fg(Color::Green);
        let wrong_part_style = Style::default().bg(Color::Black).fg(Color::Red);
        let incomplete_part_style = Style::default().bg(Color::Black).fg(Color::White);
        let future_word_style = Style::default().fg(Color::Gray);

        fn lens_to_ranges(state: &mut usize, &len: &usize) -> Option<Range<usize>> {
            let begin = std::mem::replace(state, *state + len);
            let end = *state;
            Some(begin..end)
        }
        let target_words = self.target_words.iter().scan(0, lens_to_ranges);
        let enterd_words = self.enterd_words.iter().scan(0, lens_to_ranges);

        /// (complete, wrong, incomplete)
        fn merge_word<'a>(target: &'a str, enterd: &'a str) -> (&'a str, &'a str, &'a str) {
            let first_non_match = izip!(target.char_indices(), enterd.char_indices())
                .find(|&((_, t), (_, u))| t != u);
            if let Some(((i, _), (j, _))) = first_non_match {
                let mut s = String::with_capacity(target.len() + enterd.len());
                s.push_str(&enterd);
                s.push_str(&target[i..]);
                (&enterd[..j], &enterd[j..], &target[i..])
            } else if target.len() >= enterd.len() {
                let j = enterd.len();
                (enterd, "", &target[j..])
            } else {
                let i = target.len();
                (target, &enterd[i..], "")
            }
        }
        fn merge_word2<'a>(target: &'a str, enterd: &'a str) -> (Cow<'a, str>, usize, usize) {
            let first_non_match = izip!(target.char_indices(), enterd.char_indices())
                .find(|&((_, t), (_, u))| t != u);
            if let Some(((i, _), (_j, _))) = first_non_match {
                let mut s = String::with_capacity(target.len() + enterd.len());
                s.push_str(&enterd);
                s.push_str(&target[i..]);
                (Cow::Owned(s), i, enterd.len())
                //     correct                   wrong          incomplete
                // [enterd[..j] == target[..i]] [enterd[j..]] [target[i...]]
                //                             ^ i == j      ^ enterd.len()
            } else if target.len() >= enterd.len() {
                (Cow::Borrowed(target), enterd.len(), enterd.len())
                // i = j = enterd.len()
                //     correct                   incomplete (extra)
                // [enterd[..j] == target[..i]] [target[i..]]
                //                             ^ i == j      ^ target.len()
            } else {
                (Cow::Borrowed(enterd), target.len(), enterd.len())
                // i = j = target.len()
                //     correct                   wrong (extra)
                // [enterd[..j] == target[..i]] [enterd[i..]]
                //                             ^ i == j      ^ enterd.len()
            }
        }

        let (lines, _) = target_words
            .zip_longest(enterd_words)
            .map(|pair| match pair {
                itertools::EitherOrBoth::Both(target, enterd) => {
                    merge_word(&self.target_str[target], &self.enterd_str[enterd])
                }
                itertools::EitherOrBoth::Left(target) => merge_word(&self.target_str[target], ""),
                itertools::EitherOrBoth::Right(_enterd) => {
                    unreachable!(STRINGS_CLEARED_BEFORE_FINISH)
                }
            })
            .fold(
                (vec![vec![]], 0),
                |(mut lines, linelen), (complete, wrong, incomplete)| {
                    let spcomplete = Span::styled(complete, completed_word_style);
                    let spwrong = Span::styled(wrong, wrong_part_style);
                    let spincomplete = Span::styled(incomplete, incomplete_part_style);

                    let wordlen = complete.len() + wrong.len() + incomplete.len();

                    let totallen = wordlen + linelen;
                    let len = if totallen > width.into() {
                        let line = lines.last_mut().unwrap();
                        if complete.len() > 0 {
                            line.push(spcomplete)
                        }
                        if wrong.len() > 0 {
                            line.push(spwrong)
                        }
                        if incomplete.len() > 0 {
                            line.push(spincomplete)
                        }
                        totallen
                    } else {
                        lines.push(vec![spcomplete, spwrong, spincomplete]);
                        wordlen
                    };
                    (lines, len)
                },
            );

        let lines = lines
            .into_iter()
            .map(|line| Spans::from(line))
            .collect_vec();
        let par = Paragraph::new(lines);
        // let par = Paragraph::new(vec![Spans::from(vec![
        //     Span::raw("Hello World\n"),
        //     Span::styled(
        //         "This is stylish",
        //         Style::default().fg(Color::Green).bg(Color::Red),
        //     ),
        // ])]);
        // let par = Paragraph::new(
        //     words
        //         .map(|line| {
        //             Spans::from(Span::styled(
        //                 line.into_iter().join(" "),
        //                 Style::default().bg(Color::White).fg(Color::Black),
        //             ))
        //         })
        //         .collect_vec(),
        // );

        f.render_widget(par.block(block), size)
    }
    fn stats_widget(&self, f: &mut Frame<Backend>, size: Rect) {
        let block = |title| {
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White))
                .border_type(BorderType::Rounded)
                .style(Style::default().bg(Color::Black))
        };

        let outer = block("Stats");
        let chunks = Layout::default()
            .direction(tui::layout::Direction::Horizontal)
            .constraints([Constraint::Length(20), Constraint::Min(0)])
            .split(outer.inner(size));

        f.render_widget(outer, size);

        if self.running == TestState::Running {
            f.render_widget(widgets::Clear, chunks[0])
        };
        f.render_widget(block("WPM"), chunks[0]);
        if self.running == TestState::Running {
            f.render_widget(widgets::Clear, chunks[1])
        };
        f.render_widget(block("Graph"), chunks[1]);
    }
}
