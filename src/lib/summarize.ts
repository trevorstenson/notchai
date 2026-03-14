export function summarizePrompt(raw: string): string {
  if (!raw) return "";
  // Collapse newlines into spaces for a single-line headline
  let text = raw.replace(/\n+/g, " ").trim();
  // Strip image/file attachment markers
  text = text.replace(/\(img\)|\[image[^\]]*\]|\(screenshot\)|<image>|!\[.*?\]\(.*?\)|look at this image[.,]?\s*/gi, "");
  // Strip markdown artifacts: headers, bold, italic, blockquotes, list markers, inline code
  text = text
    .replace(/^#+\s*/g, "")
    .replace(/\*\*(.+?)\*\*/g, "$1")
    .replace(/\*(.+?)\*/g, "$1")
    .replace(/__(.+?)__/g, "$1")
    .replace(/_(.+?)_/g, "$1")
    .replace(/^>\s*/g, "")
    .replace(/^[-*]\s+/g, "")
    .replace(/`([^`]+)`/g, "$1");
  // Strip common filler prefixes (case-insensitive)
  text = text
    .replace(
      /^(?:hey[,\s]*|hi[,\s]*|please[,\s]*|can you[,\s]*|could you[,\s]*|help me[,\s]*|i need you to[,\s]*|i want you to[,\s]*|i'd like you to[,\s]*)+/i,
      "",
    )
    .trim();
  // Collapse redundant whitespace and clean up trailing/leading punctuation artifacts
  text = text.replace(/\s{2,}/g, " ").replace(/^[,.\s]+|[,\s]+$/g, "").trim();
  // Capitalize first letter
  if (text.length > 0) {
    text = text.charAt(0).toUpperCase() + text.slice(1);
  }
  return text;
}
