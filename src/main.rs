use std::path::Path;

use gpui::{
    auto, div, img, prelude::*, px, rgb, size, AbsoluteLength, AnyEntity, AnyView, App,
    Application, Bounds, Context, DefiniteLength, Div, Entity, FontWeight, Length, Pixels, Point,
    Rems, SharedString, SharedUri, Window, WindowBounds, WindowOptions,
};

use tribles::{id_hex, prelude::*};

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd, TextMergeStream};
use uuid::Uuid;

use log::{debug, info, LevelFilter};
use simple_logger::SimpleLogger;

const CONTENT_WIDTH: gpui::Pixels = px(740.);

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

fn md(input: &str) -> MarkdownCell {
    MarkdownCell {
        source: SharedString::new(input),
    }
}

struct MarkdownCell {
    source: SharedString,
}

impl Render for MarkdownCell {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let events = TextMergeStream::new(Parser::new(&self.source));

        let mut stack: Vec<Div> = vec![div().flex().flex_col().w_full()];

        for event in events {
            match event {
                Event::Start(Tag::Paragraph) => {
                    stack.push(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .whitespace_normal()
                            .max_w_full()
                            .font_family("Atkinson Hyperlegible Next"),
                    );
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
                            .text_color(rgb(0x301934))
                            .font_family("Lora"),
                    );
                }
                Event::Start(Tag::BlockQuote(block_quote_kind)) => {
                    stack.push(div().flex().flex_row().flex_wrap());
                }
                Event::Start(Tag::CodeBlock(code_block_kind)) => {
                    stack.push(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .bg(rgb(0xd9d4da))
                            .font_family("Atkinson Hyperlegible Mono"),
                    );
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
                    stack.push(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .text_color(rgb(0x8b00c9))
                            .underline(),
                    );
                }
                Event::Start(Tag::Image {
                    link_type: _,
                    dest_url,
                    title: _,
                    id: _,
                }) => {
                    let path: &str = &dest_url;
                    let path: &Path = Path::new(path);
                    let img = img(path).w(px(500.)).h_auto().mb(px(16.));
                    stack.push(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .w_full()
                            .max_w_full()
                            .overflow_hidden()
                            .text_center()
                            .text_color(rgb(0x928393))
                            .font_family("Lora")
                            .child(img),
                    );
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
                    let parent = stack
                        .pop()
                        .unwrap()
                        .child(div().max_w_full().min_w_auto().child(cow_str.to_string()));
                    stack.push(parent);
                }
                Event::Code(cow_str) => {
                    let parent = stack.pop().unwrap().child(
                        div()
                            .max_w_full()
                            .min_w_auto()
                            .text_bg(rgb(0xd9d4da))
                            .font_family("Atkinson Hyperlegible Mono")
                            .child(cow_str.to_string()),
                    );
                    stack.push(parent)
                }
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
}

struct Notebook {
    cells: Vec<AnyView>,
}

impl Render for Notebook {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .text_color(rgb(0x301934))
            .bg(rgb(0xffffff))
            .p(px(16.))
            .child(
                div()
                    .id(Into::<Uuid>::into(id_hex!(
                        "4FF58F0B0FDBBC8472C5C64C9061618F"
                    )))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .items_center()
                    .max_w(CONTENT_WIDTH)
                    .min_h_full()
                    .children(self.cells.iter().cloned()),
            )
    }
}

fn main() {
    SimpleLogger::new().init().unwrap();

    Application::new().run(|cx: &mut App| {
        let cell = cx.new(|_| {
            md("# GORBIE!
This is **GORBIE!**, a _minimalist_ notebook environment for **Rust**!

Part of the [trible.space](https://trible.space) project.

![an image of 'GORBIE!' the cute alien blob and mascot of this project](./assets/gorbie.png)

# Intro

Hello Vanja!

```
// This is the main function.
fn main() {
    // Statements here are executed when the compiled binary is called.

    // Print text to the console.
    println!(\"Hello World!\");
}
```

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
")
        });

        let notebook = cx.new(|_| Notebook {
            cells: vec![cell.into()],
        });

        cx.text_system()
        .add_fonts(
            vec![
                // Lora
                include_bytes!("../assets/fonts/Lora/static/Lora-Regular.ttf").into(),
                include_bytes!("../assets/fonts/Lora/static/Lora-Italic.ttf").into(),
                include_bytes!("../assets/fonts/Lora/static/Lora-Medium.ttf").into(),
                include_bytes!("../assets/fonts/Lora/static/Lora-MediumItalic.ttf").into(),
                include_bytes!("../assets/fonts/Lora/static/Lora-Semibold.ttf").into(),
                include_bytes!("../assets/fonts/Lora/static/Lora-SemiboldItalic.ttf").into(),
                include_bytes!("../assets/fonts/Lora/static/Lora-Bold.ttf").into(),
                include_bytes!("../assets/fonts/Lora/static/Lora-BoldItalic.ttf").into(),
                // Atkinson
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/static/AtkinsonHyperlegibleNext-SemiBoldItalic.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/static/AtkinsonHyperlegibleNext-Bold.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/static/AtkinsonHyperlegibleNext-BoldItalic.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/static/AtkinsonHyperlegibleNext-ExtraBold.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/static/AtkinsonHyperlegibleNext-ExtraBoldItalic.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/static/AtkinsonHyperlegibleNext-ExtraLight.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/static/AtkinsonHyperlegibleNext-ExtraLightItalic.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/static/AtkinsonHyperlegibleNext-Italic.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/static/AtkinsonHyperlegibleNext-Light.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/static/AtkinsonHyperlegibleNext-LightItalic.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/static/AtkinsonHyperlegibleNext-Medium.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/static/AtkinsonHyperlegibleNext-MediumItalic.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/static/AtkinsonHyperlegibleNext-Regular.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/static/AtkinsonHyperlegibleNext-SemiBold.ttf").into(),
                // Atkinson Mono
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Mono/static/AtkinsonHyperlegibleMono-SemiBoldItalic.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Mono/static/AtkinsonHyperlegibleMono-Bold.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Mono/static/AtkinsonHyperlegibleMono-BoldItalic.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Mono/static/AtkinsonHyperlegibleMono-ExtraBold.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Mono/static/AtkinsonHyperlegibleMono-ExtraBoldItalic.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Mono/static/AtkinsonHyperlegibleMono-ExtraLight.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Mono/static/AtkinsonHyperlegibleMono-ExtraLightItalic.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Mono/static/AtkinsonHyperlegibleMono-Italic.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Mono/static/AtkinsonHyperlegibleMono-Light.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Mono/static/AtkinsonHyperlegibleMono-LightItalic.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Mono/static/AtkinsonHyperlegibleMono-Medium.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Mono/static/AtkinsonHyperlegibleMono-MediumItalic.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Mono/static/AtkinsonHyperlegibleMono-Regular.ttf").into(),
                include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Mono/static/AtkinsonHyperlegibleMono-SemiBold.ttf").into(),
            ])
        .unwrap();

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
                window_min_size: Some(size(px(600.), px(370.))),
                ..Default::default()
            },
            |_, cx| notebook,
        )
        .unwrap();
    });
}
