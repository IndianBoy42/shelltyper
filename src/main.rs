mod dict;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use itertools::{izip, Itertools};
use rand::distributions::Uniform;
use rand::prelude::Distribution;
use rand::thread_rng;
use std::borrow::Cow;
use std::fmt::format;
use std::io::stdout;
use std::ops::Range;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};
use tui::layout::{Constraint, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::text::{Span, Spans};
use tui::widgets::{
    self, Axis, Block, BorderType, Borders, Chart, Dataset, GraphType, LineGauge, Paragraph, Wrap,
};
use tui::{backend::CrosstermBackend, Terminal};
use tui::{symbols, Frame};

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

    Ok(())
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
    prev_hist: Instant,
    now: Instant,
    wpm: f64,
    accuracy: f64,
    accuracy_history: Vec<(f64, f64)>,
    wpm_history: Vec<(f64, f64)>,
    progress: f64,
}
impl App {
    fn new() -> App {
        let mut app = App {
            target_type: TargetStringType::Infinite,
            target_str: String::new(),
            enterd_str: String::new(),
            target_words: Vec::new(),
            enterd_words: Vec::new(),
            running: TestState::Pre,
            start: Instant::now(),
            prev_hist: Instant::now(),
            now: Instant::now(),
            wpm: 0.,
            accuracy: 0.,
            accuracy_history: Vec::with_capacity(100),
            wpm_history: Vec::with_capacity(100),
            progress: 0.,
        };
        app.new_target_string(TargetStringType::Infinite);
        app
    }

    fn new_target_string(&mut self, ty: TargetStringType) {
        let words = match ty {
            TargetStringType::Infinite => 30, // TODO:LOL
            TargetStringType::Words(n) => n,
        };
        let mut rng = thread_rng();
        let dict = dict::ENGLISH;
        let choose = Uniform::from(0..dict.len());
        self.target_str = (0..words).map(|_| dict[choose.sample(&mut rng)]).join(" ");

        self.enterd_str = String::with_capacity(self.target_str.len());
        self.target_words = self
            .target_str
            .char_indices()
            .filter(|&(_, c)| c == ' ')
            .map(|(i, _)| i + 1)
            .collect();
        // self.target_words.push(self.target_words.len());
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

            let tws = self.get_target_words().collect_vec();
            let ews = self.get_enterd_words().collect_vec();
            let (correct, total) = izip!(tws, ews).fold((0, 0), |(corr, tot), (t, e)| {
                (
                    corr + if t == e { 1 } else { 0 },
                    tot + if t.len() == 0 { 0 } else { 1 },
                )
            });
            let (correct, total) = (correct as f64, total as f64);
            let tspan = (self.now - self.start).as_secs_f64() / 60.;
            self.accuracy = correct * 100. / total; // FIXME: accuracy has some weirdness
            self.progress = total * 100. / self.target_words.len() as f64;
            self.wpm = correct / tspan;
            if (self.now - self.prev_hist).as_millis() > 100 {
                self.accuracy_history.push((self.progress, self.accuracy));
                self.wpm_history.push((self.progress, self.wpm));

                self.prev_hist = Instant::now();
            }
        }
        Ok(())
    }

    fn on_key(&mut self, key: KeyCode) -> Result<(), Box<dyn std::error::Error>> {
        let ok = match key {
            KeyCode::Char(' ') | KeyCode::Right | KeyCode::Enter => {
                if self.running == TestState::Running {
                    if self.enterd_str.chars().last() != Some(' ') {
                        self.enterd_str.push(' ');
                        *self.enterd_words.last_mut().unwrap() += 1;
                        if self.enterd_words.len() == self.target_words.len() {
                            self.running = TestState::Post;
                        } else {
                            self.enterd_words.push(*self.enterd_words.last().unwrap());
                        }
                    }
                }
            }
            KeyCode::Char(c) => {
                if self.running != TestState::Post {
                    self.enterd_str.push(c);
                    *self.enterd_words.last_mut().unwrap() = self.enterd_str.len();
                    if self.running == TestState::Pre {
                        self.running = TestState::Running;
                        self.start = Instant::now();
                        self.now = self.start;
                        self.prev_hist = self.start;
                        self.accuracy_history.clear();
                    }
                }
            }
            KeyCode::Backspace => {
                match self.enterd_str.pop() {
                    Some(' ') => {
                        self.enterd_str.push(' ');
                    }
                    Some(_) => {
                        *self.enterd_words.last_mut().unwrap() -= 1;
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
            KeyCode::Tab => {
                self.running = TestState::Post;
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
                Constraint::Length(3 + 10),
                Constraint::Min(3 + 3),
            ])
            .split(f.size());

        self.title_widget(f, chunks[0]);
        self.stats_widget(f, chunks[1]);
        self.text_widget(f, chunks[2]);
    }

    fn get_target_words(&self) -> impl Iterator<Item = &str> {
        self.target_words
            .iter()
            .scan(0, lens_to_ranges)
            .map(move |rng| &self.target_str[rng])
    }
    fn get_enterd_words(&self) -> impl Iterator<Item = &str> {
        self.enterd_words
            .iter()
            .scan(0, lens_to_ranges)
            .map(move |rng| &self.enterd_str[rng])
    }

    fn title_widget(&self, f: &mut Frame<Backend>, size: Rect) {
        let par = Paragraph::new(vec![Spans::from(vec![
            Span::raw("Hello World "),
            Span::styled(
                format!(
                    "{}",
                    match self.running {
                        TestState::Pre => {
                            "Ready to Go"
                        }
                        TestState::Running => {
                            "Test Running"
                        }
                        TestState::Post => {
                            "Test Complete"
                        }
                    }
                ),
                Style::default().fg(Color::Green).bg(Color::Red),
            ),
        ])])
        .wrap(Wrap { trim: false });
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

        let completed_word_style = Style::default().bg(Color::Black).fg(Color::White);
        let completed_part_style = Style::default().bg(Color::Black).fg(Color::Green);
        let wrong_part_style = Style::default().bg(Color::Black).fg(Color::Red);
        let incomplete_part_style = Style::default().bg(Color::Black).fg(Color::DarkGray);
        let future_word_style = Style::default().fg(Color::Gray);

        let target_words = self.target_words.iter().scan(0, lens_to_ranges);
        let enterd_words = self.enterd_words.iter().scan(0, lens_to_ranges);

        // TODO: wrapping might be able to be done by this
        // https://docs.rs/tui/0.15.0/tui/widgets/struct.Wrap.html

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
                    let spcomplete = Span::styled(
                        complete,
                        if wrong.len() == 0 && incomplete.len() == 0 {
                            completed_word_style
                        } else {
                            completed_part_style
                        },
                    );
                    let spwrong = Span::styled(wrong, wrong_part_style);
                    let spincomplete = Span::styled(incomplete, incomplete_part_style);

                    let wordlen = complete.len() + wrong.len() + incomplete.len();

                    let totallen = wordlen + linelen;
                    let len = if totallen < width.into() || false {
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

        // lines.push(Spans::from(Span::raw(target_words_dbg)));

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

        {
            if self.running == TestState::Running {
                f.render_widget(widgets::Clear, chunks[0])
            };
            let frame = block("WPM");
            let par = Paragraph::new(vec![
                Spans::from(Span::raw(format!("{:.0}", self.wpm))), //
                Spans::from(Span::raw(format!("{:.0}%", self.accuracy))), //
                Spans::from(Span::raw(format!("{:.0}%", self.progress))), //
            ])
            .block(frame)
            .wrap(Wrap { trim: false });
            f.render_widget(par, chunks[0]);
        }

        {
            if self.running == TestState::Running {
                f.render_widget(widgets::Clear, chunks[1])
            };

            let oarea = chunks[1];
            let frame = block("Graph");
            let area = frame.inner(oarea);

            let chunks = Layout::default()
                .direction(tui::layout::Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(0)])
                .split(area);

            let progress = LineGauge::default()
                //.block(Block::default().borders(Borders::ALL).title("Progress"))
                .gauge_style(
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
                .line_set(symbols::line::THICK)
                .ratio(self.progress / 100.);
            f.render_widget(progress, chunks[0]);

            let datasets = vec![
                Dataset::default()
                    .name("accuracy")
                    .marker(symbols::Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(Style::default().fg(Color::Magenta))
                    .data(&self.accuracy_history),
                Dataset::default()
                    .name("wpm")
                    .marker(symbols::Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(Style::default().fg(Color::Cyan))
                    .data(&self.wpm_history),
            ];
            let line_graph = Chart::new(datasets)
                .x_axis(
                    Axis::default()
                        // .title(Span::styled("X Axis", Style::default().fg(Color::Red)))
                        .style(Style::default().fg(Color::White))
                        .bounds([0.0, 100.0])
                        .labels(
                            ["0.0", "50.0", "100.0"]
                                .iter()
                                .cloned()
                                .map(Span::from)
                                .collect(),
                        ),
                )
                .y_axis(
                    Axis::default()
                        // .title(Span::styled("Y Axis", Style::default().fg(Color::Red)))
                        .style(Style::default().fg(Color::White))
                        .bounds([0.0, 100.0])
                        .labels(
                            ["0.0", "50.0", "100.0"]
                                .iter()
                                .cloned()
                                .map(Span::from)
                                .collect(),
                        ),
                );
            f.render_widget(line_graph, chunks[1]);

            f.render_widget(frame, oarea);
        }
    }
}

/// (complete, wrong, incomplete)
fn merge_word<'a>(target: &'a str, enterd: &'a str) -> (&'a str, &'a str, &'a str) {
    let first_non_match =
        izip!(target.char_indices(), enterd.char_indices()).find(|&((_, t), (_, u))| t != u);
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

fn lens_to_ranges(start: &mut usize, &end: &usize) -> Option<Range<usize>> {
    Some(std::mem::replace(start, end)..end)
}
