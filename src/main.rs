use gpui::{
    auto, div, prelude::*, px, rgb, size, AbsoluteLength, App, Application, Bounds, Context,
    DefiniteLength, Div, FontWeight, Pixels, Point, Rems, SharedString, Window, WindowBounds,
    WindowOptions,
};

use tribles::{id_hex, prelude::*};

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd, TextMergeStream};
use uuid::Uuid;

fn heading_size(level: HeadingLevel) -> Rems {
    match level {
        HeadingLevel::H1 => Rems(2.0),
        HeadingLevel::H2 => Rems(1.5),
        HeadingLevel::H3 => Rems(1.17),
        HeadingLevel::H4 => Rems(1.),
        HeadingLevel::H5 => Rems(0.83),
        HeadingLevel::H6 => Rems(0.67),
    }
}

fn md(input: &str) -> Div {
    let events = TextMergeStream::new(Parser::new(input));

    let mut stack: Vec<Div> = vec![div().flex().flex_col().w_full()];

    for event in events {
        match event {
            Event::Start(Tag::Paragraph) => {
                stack.push(div().flex().flex_row().flex_wrap().whitespace_normal().max_w_full().border_color(rgb(0x0000ff)));
            }
            Event::Start(Tag::Heading {
                level,
                id: _,
                classes: _,
                attrs: _,
            }) => {
                stack.push(
                    div()
                        .max_w_full()
                        .text_size(heading_size(level))
                        .text_color(rgb(0x111827)),
                );
            }
            Event::Start(Tag::BlockQuote(block_quote_kind)) => {
                stack.push(div().flex().flex_row().flex_wrap());
            }
            Event::Start(Tag::CodeBlock(code_block_kind)) => {
                stack.push(div().flex().flex_row().flex_wrap());
            }
            Event::Start(Tag::HtmlBlock) => {
                stack.push(div().flex().flex_row().flex_wrap());
            }
            Event::Start(Tag::List(_)) => {
                stack.push(div().flex().flex_col());
            }
            Event::Start(Tag::Item) => {
                stack.push(div().flex().flex_row().flex_wrap());
            }
            Event::Start(Tag::FootnoteDefinition(cow_str)) => {
                stack.push(div().flex().flex_row().flex_wrap());
            }
            Event::Start(Tag::DefinitionList) => {
                stack.push(div().flex().flex_col());
            }
            Event::Start(Tag::DefinitionListTitle) => {
                stack.push(div().flex().flex_row().flex_wrap());
            }
            Event::Start(Tag::DefinitionListDefinition) => {
                stack.push(div().flex().flex_row().flex_wrap());
            }
            Event::Start(Tag::Table(alignments)) => {
                stack.push(div().flex().flex_col());
            }
            Event::Start(Tag::TableHead) => {
                stack.push(div().flex().flex_row().flex_wrap());
            }
            Event::Start(Tag::TableRow) => {
                stack.push(div().flex().flex_row().flex_wrap());
            }
            Event::Start(Tag::TableCell) => {
                stack.push(div().flex().flex_row().flex_wrap());
            }
            Event::Start(Tag::Emphasis) => {
                stack.push(div().italic());
            }
            Event::Start(Tag::Strong) => {
                stack.push(div().font_weight(FontWeight::BOLD));
            }
            Event::Start(Tag::Strikethrough) => {
                stack.push(div().line_through());
            }
            Event::Start(Tag::Superscript) => {
                stack.push(div().flex().flex_row().flex_wrap());
            }
            Event::Start(Tag::Subscript) => {
                stack.push(div().flex().flex_row().flex_wrap());
            }
            Event::Start(Tag::Link {
                link_type: _,
                dest_url: _,
                title: _,
                id: _,
            }) => {
                stack.push(div().flex().flex_row().flex_wrap());
            }
            Event::Start(Tag::Image {
                link_type: _,
                dest_url: _,
                title: _,
                id: _,
            }) => {
                stack.push(div().flex().flex_row().flex_wrap());
            }
            Event::Start(Tag::MetadataBlock(_metadata_block_kind)) => {
                stack.push(div().flex().flex_row().flex_wrap());
            }
            Event::End(_tag) => {
                if let Some(child) = stack.pop() {
                    if let Some(parent) = stack.pop() {
                        stack.push(parent.child(child));
                    } else {
                        return child;
                    }
                }
            }
            Event::Text(cow_str) => {
                let parent = stack.pop().unwrap().child(div().max_w_full().child(cow_str.to_string()));
                stack.push(parent);
            }
            Event::Code(cow_str) => {}
            Event::InlineMath(cow_str) => {}
            Event::DisplayMath(cow_str) => {}
            Event::Html(cow_str) => {}
            Event::InlineHtml(cow_str) => {}
            Event::FootnoteReference(cow_str) => {}
            Event::SoftBreak => {}
            Event::HardBreak => {}
            Event::Rule => {}
            Event::TaskListMarker(_) => {}
        }
    }

    stack
        .pop()
        .unwrap_or(div().child("Failed to parse markdown."))
}
struct Notebook {}

impl Render for Notebook {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .debug_below()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .text_color(rgb(0x111827))
            .bg(rgb(0xffffff))
            .child(
        div()
            .id(Into::<Uuid>::into(id_hex!("4FF58F0B0FDBBC8472C5C64C9061618F")))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .max_w(px(740.))
            .min_h_full()
            .child(md(
"# GORBIE!
This is **GORBIE!**, a _minimalist_ notebook environment for Rust!

Part of the [trible.space](https://trible.space) project.

![trible.space](./assets/gorbie.png)

# Intro

Hello Vanja!

Lorem ipsum dolor sit amet, consectetur adipiscing elit.\
Vestibulum commodo purus ac arcu dapibus, quis scelerisque lacus pretium.\
Curabitur convallis ultrices neque. Ut lobortis non urna porttitor faucibus.\
Quisque blandit a urna a malesuada. Proin a convallis ipsum.\
Aliquam vitae nibh mi. Etiam tempor molestie bibendum.\
Suspendisse volutpat lorem eget ex sollicitudin, quis suscipit metus ultricies.\
Nam varius sem dapibus mi lobortis eleifend.\
Nulla pellentesque eros vel semper fringilla.\
Quisque facilisis tortor eu diam pharetra consectetur.\
Interdum et malesuada fames ac ante ipsum primis in faucibus.\
Donec imperdiet, quam at ornare sollicitudin, justo augue tincidunt purus,\
quis ultrices sapien nibh ac massa.

Sed egestas, risus sed sagittis ullamcorper, nisi eros aliquam elit,\
id posuere orci nulla sit amet nisi. Donec leo magna, lobortis at imperdiet vel,\
finibus quis massa. Cras a arcu neque. Pellentesque aliquet vehicula convallis.\
Aliquam erat volutpat. Nulla luctus justo tellus, sed mollis elit rhoncus ut.\
Aliquam sodales dui arcu, sed egestas ex eleifend eu. Donec eu tellus erat.\
Proin tincidunt felis metus, sit amet tempus eros semper at.\
Aenean in turpis tortor. Integer ut nibh a massa maximus bibendum.\
Praesent sodales eu felis sed vehicula. Donec condimentum efficitur sodales.

Pellentesque eleifend felis euismod, convallis arcu vel, dignissim augue.\
Praesent eu lorem mauris. Sed non posuere ligula.\
Vestibulum faucibus venenatis interdum. Donec dui quam, mattis ac suscipit et,\
tristique at velit. In at auctor justo, eu auctor metus.\
Aenean ut elementum lorem. Fusce vel metus eros. Nulla facilisi.\
Quisque elementum interdum laoreet. Ut sit amet sapien pellentesque,\
tempus turpis quis, blandit felis. Nunc feugiat lacinia nisi a tempus.\
Praesent dictum aliquam ligula. Vestibulum et sapien nisi.\
Aenean pretium turpis a velit tristique rutrum.
"
            )))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let upper_left = Point {
            x: Pixels(0.),
            y: Pixels(0.),
        };
        let bottom_right = Point {
            x: Pixels(600.),
            y: Pixels(800.),
        };
        let bounds = Bounds::from_corners(upper_left, bottom_right);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Notebook {}),
        )
        .unwrap();
    });
}
