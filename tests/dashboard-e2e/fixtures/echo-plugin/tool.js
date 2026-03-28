let input = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  input += chunk;
});
process.stdin.on("end", () => {
  let request = {};
  try {
    request = input.trim() ? JSON.parse(input) : {};
  } catch (_) {
    request = {};
  }
  const text = request.arguments && typeof request.arguments.text === "string"
    ? request.arguments.text
    : "empty";
  process.stdout.write(JSON.stringify({ ok: true, content: `echo:${text}` }));
});
