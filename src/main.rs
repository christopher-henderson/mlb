#[macro_use]
extern crate lazy_static;

use image::{ImageFormat, RgbaImage};
use piston_window::{EventLoop, Glyphs, ReleaseEvent, Transformed};
use std::process::exit;

mod api;
mod lineup;

use lineup::*;

// I gotta say, I was ecstatic the first time I ever found out that include_bytes/str was a thing.
// I have long hated the bundling of loose assets and little file extras into what is suppose
// to be a small, portable (in both the ARCH/OS sense as well as the common sense), app.
static BACKGROUND_BYTES: &[u8] = include_bytes!("../assets/background.jpg");
static LEFT_ARROW_BYTES: &[u8] = include_bytes!("../assets/left_arrow.png");
static RIGHT_ARROW_BYTES: &[u8] = include_bytes!("../assets/right_arrow.png");
static FONT: &[u8] = include_bytes!("../assets/OpenSans-Bold.ttf");

static BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
static WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
// The padding between onscreen game snippets.
static PADDING: f64 = 27.5;

lazy_static! {
    static ref BACKGROUND: RgbaImage =
        image::load_from_memory_with_format(BACKGROUND_BYTES, ImageFormat::JPEG)
            .unwrap()
            .into_rgba();
    static ref LEFT_ARROW: RgbaImage =
        image::load_from_memory_with_format(LEFT_ARROW_BYTES, ImageFormat::PNG)
            .unwrap()
            .into_rgba();
    static ref RIGHT_ARROW: RgbaImage =
        image::load_from_memory_with_format(RIGHT_ARROW_BYTES, ImageFormat::PNG)
            .unwrap()
            .into_rgba();
}

// I was torn on this dependency. From and SDK perspective, know that I understand that making
// the decision to embed a runtime into client code is a hefty decision. Not necessarily wrong,
// but it's not necessarily a light-hearted thing to do either.
//
// HOWEVER, this isn't an embedded SDK and I have a transitive dependency on tokio via hyper
// anyways, so I may as well bask in the async goodness.
#[tokio::main]
async fn main() {
    // Well, I know the name of the org I'm interviewing with. So I've got that going for me.
    let title = "Disney Streaming Services";
    // I chose piston simply because my quick experimentation with other libraries, such as glium,
    // asked me to write GLSL code and feed that into macros for consumption by OpenGL. I don't
    // need vectors and shading and all that jazz, I just needed a 2D window.
    //
    // I also admit that my use of this library can be rather repetitive. This is not my usual
    // style of programming as of course you want typical use cases to resolve down to shared
    // functionality. However, this requires that you fundamentally understand what your underlying
    // dependency is asking of you as well of its general philosophies. I pulled this library off
    // the shelf so...sorry, my use of it is rather blunt.
    let mut window: piston_window::PistonWindow =
        piston_window::WindowSettings::new(title, [1920, 1080])
            .exit_on_esc(true)
            .build()
            .unwrap_or_else(|e| panic!("Failed to build PistonWindow: {}", e));
    // We're going to be using this context repeatedly in each loop.
    // Calling something a ThingContext that takes in ThingFactory is so library specific and
    // mysterious that I admit that I do not understand the original intent here. I have
    // heard this called "homeopathic naming" - the notion that the more you dilute the naming
    // the more meaningful it becomes (an engineer has a problem, she decides to use Java,
    // she now has an AbstractObserverFactoryImpl).
    //
    // My use of this library was purely a panic to find any reasonable 2D graphics library
    // that could see me through this ordeal. So I admit that this is a case of satisfying the API
    // without any real deep understanding of what they are asking of me here.
    let mut ctx = piston_window::TextureContext {
        factory: window.factory.clone(),
        encoder: window.factory.create_command_buffer().into(),
    };
    let fullscreen = graphics::image::Image::new().rect([0.0, 0.0, 1920.0, 1080.0]);
    let background: piston_window::G2dTexture = piston_window::Texture::from_image(
        &mut ctx,
        &(*BACKGROUND),
        &piston_window::TextureSettings::new(),
    )
    .unwrap();
    // This is me TRYING to make this a bit more efficient. The downside of using this easy 2D
    // library is that I have apparently inherited a rather inefficient event loop
    // (see https://github.com/PistonDevelopers/piston/issues/1109). Frankly, I should NOT be
    // consuming 50MB of RAM and nearly 1-2% of CPU, but firing up this event loop on even
    // a completely blank screen will force me into that consumption, and that is unfortunate.
    //
    // However, limiting the frame rate cuts the CPU usage (on my box) down to under 1% at least.
    // This framerate seemed like a fair emulation of how quickly these sorts of menus tend
    // to render on actual TVs.
    window.set_max_fps(10);
    // It's kind of a useless thing to .await immediately upon application startup as it is
    // blocking the window from rendering. I stretched for having the photos load
    // asynchronously, however getting that initial API call to load in the background as well
    // would have been a bit much for such a short time frame. Backlog candidate.
    let mut schedule: Schedule = match api::Schedule::try_from(api::DEFAULT).await {
        Ok(schedule) => schedule.into(),
        // I handle the error of not being able to pull the initial API call and render
        // as the sole text onto the screen. A restart is required to try again. I admit
        // that after this, any Result given back by the graphics library I just unwrap. This
        // is because after this point everything is already in memory so we're not suffering
        // from IO failures, however it is entirely possible that we were given back, say,
        // images that don't parse out correctly. I simply did not have the time to scope
        // out such rich error handling and how that would tie into the main window rendering.
        //
        // Other parts of this application that are more in my problem domain I am more careful with.
        //
        // I am aware that the text needs to be wrapped around as the error messages fall
        // off the screen. Wrapping text into columns is not difficult, however you have
        // to handle the newlines manually within this text renderer which I did not have
        // the time to do. Some of the snippet subheaders suffer from this same problem.
        Err(err) => display_err(err, window, background),
    };
    // Glyphs are the font cache that we will be using for this application.
    //
    // It's a shame, I found a cool open source font that looked very much like that blocky
    // MLB sans serif font, however it has a very anemic selection of symbols and just looked
    // back when dealing with non-alpha text.
    let mut glyphs = Glyphs::from_bytes(
        FONT,
        piston_window::TextureContext {
            factory: window.factory.clone(),
            encoder: window.factory.create_command_buffer().into(),
        },
        piston_window::TextureSettings::new(),
    )
    .unwrap();
    while let Some(e) = window.next() {
        // Move the cursor on key-up events. I would kinda like to implement fast scrolling
        // via long key holds. But alas, into the backlog it goes.
        match e.release_args() {
            Some(piston_window::Button::Keyboard(piston_window::Key::Left)) => {
                schedule.left();
            }
            Some(piston_window::Button::Keyboard(piston_window::Key::Right)) => {
                schedule.right();
            }
            _ => (),
        };
        window.draw_2d(&e, |c, g, device| {
            // This is the main rendering loop as per piston convention.
            //
            // I admit that these X/Y transformations are more of a result
            // of me experimenting around to get an orientation on the page
            // and seeing what works aesthetically. I did do some manual computations
            // to get an idea of where these objects should lay on the screen.
            // However, by and large, I am admitting that this applications is not
            // "responsive" in the sense that it does not respond to different sizes.
            // In Agile terms, I reckon that I would put that work onto the next sprint.
            piston_window::clear(BLACK, g);
            fullscreen.draw(&background, &graphics::DrawState::default(), c.transform, g);
            // The first item is padded from the left most wall of the screen.
            let mut left_edge = PADDING;
            // And the right edge is computed as the left_edge plus
            // whatever the width of the image is.
            let mut right_edge: f64;
            for item in schedule.page() {
                match item {
                    Snippet::Large(image, heading, subheading) => {
                        right_edge = left_edge + image.width() as f64;
                        let rect = graphics::image::Image::new().rect([
                            0.0,
                            0.0,
                            image.width() as f64,
                            image.height() as f64,
                        ]);
                        let txt = piston_window::Texture::from_image(
                            &mut ctx,
                            image,
                            &piston_window::TextureSettings::new(),
                        )
                        .unwrap();
                        rect.draw(
                            &txt,
                            &graphics::DrawState::default(),
                            c.transform.trans(left_edge, 540.0),
                            g,
                        );
                        // Render our header and subheader
                        piston_window::text(
                            WHITE,
                            16,
                            heading,
                            &mut glyphs,
                            c.transform.trans(left_edge + 40.0, 500.0),
                            g,
                        )
                        .unwrap();
                        piston_window::text(
                            WHITE,
                            16,
                            subheading,
                            &mut glyphs,
                            c.transform.trans(left_edge, 855.0),
                            g,
                        )
                        .unwrap();
                        // And I guess we have to...flush the font encoder with the given device?
                        // This object graph doesn't make much sense to me, but that just
                        // might be because I don't know anything about graphics.
                        glyphs.factory.encoder.flush(device);
                    }
                    Snippet::Small(image) => {
                        right_edge = left_edge + image.width() as f64;
                        let rect = graphics::image::Image::new().rect([
                            0.0,
                            0.0,
                            image.width() as f64,
                            image.height() as f64,
                        ]);
                        let txt = piston_window::Texture::from_image(
                            &mut ctx,
                            image,
                            &piston_window::TextureSettings::new(),
                        )
                        .unwrap();
                        rect.draw(
                            &txt,
                            &graphics::DrawState::default(),
                            c.transform.trans(left_edge, 578.5),
                            g,
                        );
                    }
                }
                // This is computing the small padding in-between snippets.
                left_edge = right_edge + PADDING;
            }
            // has_less and has_more describe whether or not there is a page to left or the right,
            // which drives the decision on whether or not to render the scroll arrow indicators.
            //
            // When you don't have enough time for large technical implementations goals
            // (such as richer error handling or window responsiveness) then you should try to
            // fill in the sprint/release with small attention to detail that often delight
            // stakeholders. These small details don't take much time, they're going to be there
            // eventually anyways, and their implementation buys you a bit more time (politically)
            // to implement the harder stuff while keeping everyone happy.
            if schedule.has_less() {
                let txt = piston_window::Texture::from_image(
                    &mut ctx,
                    &*LEFT_ARROW,
                    &piston_window::TextureSettings::new(),
                )
                .unwrap();
                let rect = graphics::image::Image::new().rect([
                    0.0,
                    0.0,
                    LEFT_ARROW.width() as f64,
                    LEFT_ARROW.height() as f64,
                ]);
                rect.draw(&txt, &graphics::DrawState::default(), c.transform, g);
            }
            if schedule.has_more() {
                let txt = piston_window::Texture::from_image(
                    &mut ctx,
                    &*RIGHT_ARROW,
                    &piston_window::TextureSettings::new(),
                )
                .unwrap();
                let rect = graphics::image::Image::new().rect([
                    0.0,
                    0.0,
                    RIGHT_ARROW.width() as f64,
                    RIGHT_ARROW.height() as f64,
                ]);
                rect.draw(
                    &txt,
                    &graphics::DrawState::default(),
                    c.transform.trans(1920.0 - RIGHT_ARROW.width() as f64, 0.0),
                    g,
                );
            }
        });
    }
}

// The error case alternative. It takes ownership of the window and displays the APIError until exit.
//
// This was sort of a noisy, last minute, function to begin with but cargo fmt really formatted
// in a way that I don't quite to boot.
//
// Which brings up a good point - style. I grab whatever is the formatter de'jure and just use it.
// Don't like how go fmt mangled your beautiful code? Deal with it, arguments and unnecessary
// differences in change requests make this a hill not worth dying on.
fn display_err(
    err: api::APIError,
    mut window: piston_window::PistonWindow,
    background: piston_window::G2dTexture,
) -> ! {
    let err_text = format!("{}", err);
    let mut glyphs = Glyphs::from_bytes(
        FONT,
        piston_window::TextureContext {
            factory: window.factory.clone(),
            encoder: window.factory.create_command_buffer().into(),
        },
        piston_window::TextureSettings::new(),
    )
    .unwrap();
    let fullscreen = graphics::image::Image::new().rect([0.0, 0.0, 1920.0, 1080.0]);
    while let Some(e) = window.next() {
        window.draw_2d(&e, |c, g, device| {
            piston_window::clear(BLACK, g);
            fullscreen.draw(&background, &graphics::DrawState::default(), c.transform, g);
            piston_window::text(
                WHITE,
                16,
                err_text.as_str(),
                &mut glyphs,
                c.transform.trans(0.0, 500.0),
                g,
            )
            .unwrap();
            glyphs.factory.encoder.flush(device);
        });
    }
    exit(1);
}
