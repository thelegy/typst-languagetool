use std::ops::Range;

use typst::{
	layout::{Abs, Em, Point},
	model::Document,
	syntax::{FileId, Source, Span, SyntaxKind},
	text::TextItem,
};

use crate::Suggestion;

pub struct Mapping {
	chars: Vec<(Span, Range<u16>)>,
}

impl Mapping {
	pub fn location(&self, suggestion: &Suggestion, source: &Source) -> Vec<Range<usize>> {
		let chars = &self.chars[suggestion.start..suggestion.end];
		let mut locations = Vec::<Range<usize>>::new();
		for (span, range) in chars.iter().cloned() {
			let Some(id) = span.id() else {
				continue;
			};
			if id != source.id() {
				continue;
			}
			let Some(node) = source.find(span) else {
				continue;
			};
			if node.kind() == SyntaxKind::Text {
				let start = node.range().start;
				let range = (start + range.start as usize)..(start + range.end as usize);
				match locations.last_mut() {
					Some(last_range) if last_range.end == range.start => last_range.end = range.end,
					_ => locations.push(range),
				}
			} else {
				let range = node.range();
				match locations.last_mut() {
					Some(last_range) if *last_range == range => {},
					_ => locations.push(range),
				}
			}
		}
		locations
	}
}
const LINE_SPACING: Em = Em::new(0.65);

pub fn document(doc: &Document, chunk_size: usize, file_id: FileId) -> Vec<(String, Mapping)> {
	let mut res = Vec::new();

	for page in &doc.pages {
		let mut converter = Converter::new(chunk_size);
		converter.frame(&page.frame, Point::zero(), &mut res, file_id);
		if converter.contains_file {
			res.push((converter.text, converter.mapping));
		}
	}
	res
}

struct Converter {
	text: String,
	mapping: Mapping,
	x: Abs,
	y: Abs,
	span: (Span, u16),
	chunk_size: usize,
	contains_file: bool,
}

impl Converter {
	fn new(chunk_size: usize) -> Self {
		Self {
			text: String::new(),
			mapping: Mapping { chars: Vec::new() },
			x: Abs::zero(),
			y: Abs::zero(),
			span: (Span::detached(), 0),
			contains_file: false,
			chunk_size,
		}
	}

	fn insert_space(&mut self) {
		self.text += " ";
		self.mapping.chars.push((Span::detached(), 0..0));
	}

	fn seperate(&mut self, res: &mut Vec<(String, Mapping)>) {
		if self.contains_file {
			let text = std::mem::take(&mut self.text);
			let mapping = std::mem::replace(&mut self.mapping, Mapping { chars: Vec::new() });
			res.push((text, mapping));
		}
		*self = Converter::new(self.chunk_size);
	}

	fn insert_parbreak(&mut self, res: &mut Vec<(String, Mapping)>) {
		if self.mapping.chars.len() > self.chunk_size {
			self.seperate(res);
			return;
		}
		self.text += "\n\n";
		self.mapping.chars.push((Span::detached(), 0..0));
		self.mapping.chars.push((Span::detached(), 0..0));
	}

	fn whitespace(&mut self, text: &TextItem, pos: Point, res: &mut Vec<(String, Mapping)>) {
		if self.x.approx_eq(pos.x) {
			return;
		}
		let line_spacing = (text.font.metrics().cap_height + LINE_SPACING).at(text.size);
		let next_line = (self.y + line_spacing).approx_eq(pos.y);
		if !next_line {
			self.insert_parbreak(res);
			return;
		}
		let span = text.glyphs[0].span;
		if span == self.span {
			return;
		}
		self.insert_space();
	}

	fn frame(
		&mut self,
		frame: &typst::layout::Frame,
		pos: Point,
		res: &mut Vec<(String, Mapping)>,
		file_id: FileId,
	) {
		for &(p, ref item) in frame.items() {
			self.item(p + pos, item, res, file_id);
		}
	}

	fn item(
		&mut self,
		pos: Point,
		item: &typst::layout::FrameItem,
		res: &mut Vec<(String, Mapping)>,
		file_id: FileId,
	) {
		use typst::introspection::Meta as M;
		use typst::layout::FrameItem as I;
		match item {
			I::Group(g) => self.frame(&g.frame, pos, res, file_id),
			I::Text(t) => {
				self.whitespace(t, pos, res);
				self.x = pos.x + t.width();
				self.y = pos.y;
				self.text += t.text.as_str();

				let mut iter = t.glyphs.iter();
				for _ in t.text.encode_utf16() {
					let g = iter.next();
					let m = g
						.map(|g| (g.span.0, g.span.1..(g.span.1 + g.range.len() as u16)))
						.unwrap_or((Span::detached(), 0..0));
					if let Some(id) = m.0.id() {
						self.span = (m.0, m.1.end);
						if id == file_id {
							self.contains_file = true;
						}
					}
					self.mapping.chars.push(m);
				}
			},
			I::Meta(M::Link(..) | M::Elem(..) | M::Hide, _) | I::Shape(..) | I::Image(..) => {},
		}
	}
}
