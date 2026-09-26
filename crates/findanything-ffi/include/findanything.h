#ifndef FINDANYTHING_H
#define FINDANYTHING_H
// Call initialize before NSApplication/instance election. Requests may block;
// send search/activation requests from one serial worker, never the UI thread.
void fa_initialize(void);
// Input is a borrowed NUL-terminated UTF-8 JSON string. Output is owned by Rust.
char *fa_request(const char *json);
// Free each non-NULL response exactly once. Do not use free()/delete on it.
void fa_string_free(char *json);
#endif
