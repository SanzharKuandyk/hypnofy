import { Node } from "@tiptap/core";

export const PauseNode = Node.create({
  name: "pause",
  group: "block",
  atom: true,
  selectable: true,
  addAttributes() {
    return { seconds: { default: 5, parseHTML: element => Number(element.getAttribute("data-seconds")) || 5 } };
  },
  parseHTML() { return [{ tag: "div[data-pause]" }]; },
  renderHTML({ HTMLAttributes }) {
    return ["div", { "data-pause": "", "data-seconds": HTMLAttributes.seconds, class: "pause-block", contenteditable: "false" }, `Pause · ${HTMLAttributes.seconds} seconds`];
  },
});
