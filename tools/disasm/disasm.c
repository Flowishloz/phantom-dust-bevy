// Disassemble a compiled D3D shader (.cso) using the system d3dcompiler_47.dll.
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct ID3DBlobVtbl {
    HRESULT (__stdcall *QueryInterface)(void*, const IID*, void**);
    ULONG (__stdcall *AddRef)(void*);
    ULONG (__stdcall *Release)(void*);
    void* (__stdcall *GetBufferPointer)(void*);
    SIZE_T (__stdcall *GetBufferSize)(void*);
} ID3DBlobVtbl;
typedef struct { ID3DBlobVtbl* v; } Blob;
typedef HRESULT (__stdcall *Disasm)(const void*, SIZE_T, UINT, const char*, Blob**);

int main(int argc, char** argv) {
    if (argc < 2) { fprintf(stderr, "usage: disasm file.cso\n"); return 1; }
    FILE* f = fopen(argv[1], "rb");
    if (!f) { perror("open"); return 1; }
    fseek(f, 0, SEEK_END); long n = ftell(f); fseek(f, 0, SEEK_SET);
    void* buf = malloc(n); fread(buf, 1, n, f); fclose(f);
    HMODULE m = LoadLibraryA("d3dcompiler_47.dll");
    if (!m) { fprintf(stderr, "no d3dcompiler_47.dll\n"); return 1; }
    Disasm d = (Disasm)GetProcAddress(m, "D3DDisassemble");
    Blob* out = NULL;
    HRESULT hr = d(buf, n, 0, NULL, &out);
    if (FAILED(hr)) { fprintf(stderr, "disassemble failed %lx\n", hr); return 1; }
    fwrite(out->v->GetBufferPointer(out), 1, out->v->GetBufferSize(out) - 1, stdout);
    return 0;
}
