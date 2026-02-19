use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use uuid::Uuid;

pub type ReaderId = Uuid;
pub type CommandId = Uuid;

#[derive(Debug, Clone)]
pub struct TimestampedSegment {
    pub timestamp: Instant,
    pub content: String,
    pub cumulative_offset: usize,
}

#[derive(Debug)]
pub struct ReaderState {
    pub id: ReaderId,
    pub last_read_at: Instant,
    pub read_cursor: usize,
}


#[derive(Debug, Clone)]
pub struct CommandExecution {
    pub id: CommandId,
    #[allow(dead_code)]
    pub command: String,
    #[allow(dead_code)]
    pub started_at: Instant,
    pub completed_at: Option<Instant>,
    pub start_offset: usize,
    pub end_offset: Option<usize>,
}

impl CommandExecution {
    pub fn new(command: String, start_offset: usize) -> Self {
        Self {
            id: Uuid::new_v4(),
            command,
            started_at: Instant::now(),
            completed_at: None,
            start_offset,
            end_offset: None,
        }
    }

    pub fn complete(&mut self, end_offset: usize) {
        self.completed_at = Some(Instant::now());
        self.end_offset = Some(end_offset);
    }
}

const DEFAULT_MAX_SEGMENTS: usize = 1000;
const DEFAULT_MAX_BYTES: usize = 10 * 1024 * 1024; // 10MB
const DEFAULT_READER_TIMEOUT: Duration = Duration::from_secs(3600); // 1 hour

pub struct OutputTracker {
    segments: VecDeque<TimestampedSegment>,
    total_bytes: usize,
    readers: HashMap<ReaderId, ReaderState>,
    commands: HashMap<CommandId, CommandExecution>,
    max_segments: usize,
    max_bytes: usize,
    reader_timeout: Duration,
    tracking_start: Instant,
}

impl OutputTracker {
    pub fn new() -> Self {
        Self {
            segments: VecDeque::new(),
            total_bytes: 0,
            readers: HashMap::new(),
            commands: HashMap::new(),
            max_segments: DEFAULT_MAX_SEGMENTS,
            max_bytes: DEFAULT_MAX_BYTES,
            reader_timeout: DEFAULT_READER_TIMEOUT,
            tracking_start: Instant::now(),
        }
    }

    pub fn record_output(&mut self, content: String) {
        if content.is_empty() {
            return;
        }

        let segment = TimestampedSegment {
            timestamp: Instant::now(),
            content: content.clone(),
            cumulative_offset: self.total_bytes,
        };

        self.total_bytes += content.len();
        self.segments.push_back(segment);

        self.enforce_limits();
    }

    fn enforce_limits(&mut self) {
        while self.segments.len() > self.max_segments {
            self.segments.pop_front();
        }

        let mut current_bytes: usize = self.segments.iter().map(|s| s.content.len()).sum();
        while current_bytes > self.max_bytes && !self.segments.is_empty() {
            if let Some(removed) = self.segments.pop_front() {
                current_bytes -= removed.content.len();
            }
        }
    }

    pub fn create_reader(&mut self) -> ReaderId {
        self.cleanup_stale_readers();

        let reader = ReaderState {
            id: Uuid::new_v4(),
            last_read_at: Instant::now(),
            read_cursor: self.total_bytes,
        };
        let id = reader.id;
        self.readers.insert(id, reader);
        id
    }

    pub fn read_new(&mut self, reader_id: ReaderId) -> Option<(String, bool)> {
        let reader = self.readers.get_mut(&reader_id)?;
        reader.last_read_at = Instant::now();

        let cursor = reader.read_cursor;
        let mut result = String::new();

        for segment in &self.segments {
            let segment_start = segment.cumulative_offset;
            let segment_end = segment_start + segment.content.len();

            if segment_end <= cursor {
                continue;
            }

            if segment_start >= cursor {
                result.push_str(&segment.content);
            } else {
                let offset_in_segment = cursor - segment_start;
                if offset_in_segment < segment.content.len() {
                    result.push_str(&segment.content[offset_in_segment..]);
                }
            }
        }

        reader.read_cursor = self.total_bytes;

        let has_more = false;
        Some((result, has_more))
    }

    pub fn stop_reader(&mut self, reader_id: ReaderId) -> bool {
        self.readers.remove(&reader_id).is_some()
    }

    fn cleanup_stale_readers(&mut self) {
        let now = Instant::now();
        let timeout = self.reader_timeout;
        self.readers
            .retain(|_, reader| now.duration_since(reader.last_read_at) < timeout);
    }

    pub fn start_command(&mut self, command: String) -> CommandId {
        let execution = CommandExecution::new(command, self.total_bytes);
        let id = execution.id;
        self.commands.insert(id, execution);
        id
    }

    pub fn complete_command(&mut self, command_id: CommandId) -> bool {
        if let Some(execution) = self.commands.get_mut(&command_id) {
            execution.complete(self.total_bytes);
            true
        } else {
            false
        }
    }

    pub fn get_command_output(&self, command_id: CommandId) -> Option<String> {
        let execution = self.commands.get(&command_id)?;
        let end_offset = execution.end_offset?;

        let start = execution.start_offset;
        let mut result = String::new();

        for segment in &self.segments {
            let segment_start = segment.cumulative_offset;
            let segment_end = segment_start + segment.content.len();

            if segment_end <= start {
                continue;
            }
            if segment_start >= end_offset {
                break;
            }

            let content_start = if segment_start < start {
                start - segment_start
            } else {
                0
            };

            let content_end = if segment_end > end_offset {
                end_offset - segment_start
            } else {
                segment.content.len()
            };

            if content_start < content_end && content_end <= segment.content.len() {
                result.push_str(&segment.content[content_start..content_end]);
            }
        }

        Some(result)
    }

    pub fn read_time_range(&self, start_ms: u64, end_ms: u64) -> String {
        let start_duration = Duration::from_millis(start_ms);
        let end_duration = Duration::from_millis(end_ms);

        let start_instant = self.tracking_start + start_duration;
        let end_instant = self.tracking_start + end_duration;

        let mut result = String::new();

        for segment in &self.segments {
            if segment.timestamp >= start_instant && segment.timestamp <= end_instant {
                result.push_str(&segment.content);
            }
        }

        result
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    pub fn tracking_start(&self) -> Instant {
        self.tracking_start
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.tracking_start.elapsed().as_millis() as u64
    }
}

impl Default for OutputTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_read() {
        let mut tracker = OutputTracker::new();
        let reader_id = tracker.create_reader();

        tracker.record_output("hello ".to_string());
        tracker.record_output("world".to_string());

        let (content, _) = tracker.read_new(reader_id).unwrap();
        assert_eq!(content, "hello world");

        tracker.record_output("!".to_string());
        let (content, _) = tracker.read_new(reader_id).unwrap();
        assert_eq!(content, "!");
    }

    #[test]
    fn test_multiple_readers() {
        let mut tracker = OutputTracker::new();

        tracker.record_output("first ".to_string());
        let reader1 = tracker.create_reader();

        tracker.record_output("second ".to_string());
        let reader2 = tracker.create_reader();

        tracker.record_output("third".to_string());

        let (content1, _) = tracker.read_new(reader1).unwrap();
        let (content2, _) = tracker.read_new(reader2).unwrap();

        assert_eq!(content1, "second third");
        assert_eq!(content2, "third");
    }

    #[test]
    fn test_command_tracking() {
        let mut tracker = OutputTracker::new();

        let cmd_id = tracker.start_command("ls -la".to_string());
        tracker.record_output("file1.txt\n".to_string());
        tracker.record_output("file2.txt\n".to_string());
        tracker.complete_command(cmd_id);

        let output = tracker.get_command_output(cmd_id).unwrap();
        assert_eq!(output, "file1.txt\nfile2.txt\n");
    }

    #[test]
    fn test_stop_reader() {
        let mut tracker = OutputTracker::new();
        let reader_id = tracker.create_reader();

        assert!(tracker.read_new(reader_id).is_some());
        assert!(tracker.stop_reader(reader_id));
        assert!(tracker.read_new(reader_id).is_none());
    }
}
