use eframe::egui;
use eframe::egui::{Context, ScrollArea, Ui};
use std::io::{BufWriter, Write};

struct CustomLogger {
   buffer: BufWriter<Vec<u8>>,
}

impl CustomLogger {
   fn new() -> Self {
      CustomLogger {
         buffer: BufWriter::new(Vec::new()),
      }
   }

   fn log(&mut self, message: &str) {
      writeln!(self.buffer, "{}", message).unwrap();
   }

   fn flush(&mut self) {
      self.buffer.flush().unwrap();
   }

   fn display_logs(&self, ui: &mut Ui) {
      let text = String::from_utf8(self.buffer.buffer().to_vec()).unwrap();
      ScrollArea::vertical()
          .show(ui, |ui| {
             ui.label(text);

             ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
          });
   }
}

fn main() {
   let mut logger = CustomLogger::new();
   logger.log("This is a log message.");
   logger.flush();

   eframe::run_native(
      "Custom Logger",
      eframe::NativeOptions::default(),
      Box::new(|_cc| Ok(Box::new(MyApp { logger, count: 0 }))),
   ).unwrap();
}

struct MyApp {
   logger: CustomLogger,
   count: u32,
}

impl eframe::App for MyApp {
   fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
      egui::Window::new("Logs")
          .resizable(true)
          .show(ctx, |ui: &mut Ui| {
             ui.label("Log Output:");
             // self.logger.log("This is a log message.");
             self.logger.display_logs(ui);
          });

      egui::Window::new("test")
          .show(ctx, |ui| {
             if ui.button("Add").clicked() {
                self.logger.log(format!("Things {}", self.count).as_str());
                self.count += 1;
             }
          });
   }
}