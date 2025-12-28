process.stdin.on('data', (data) => {
  try {
    const result = eval(data.toString());
    process.stdout.write(result + "\n");
  } catch (e) {
    process.stdout.write(JSON.stringify({ ok: false, error: e.message }) + "\n");
  }
});
