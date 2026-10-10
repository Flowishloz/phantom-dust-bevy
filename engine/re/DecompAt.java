// Decompile functions at given addresses, creating them first if Ghidra's
// auto-analysis missed them (e.g. handlers only reached through a pointer table).
// Our own tool. Output is decompiled game code: keep it outside the repo.
//
// Usage (headless, -readOnly so the project is not changed):
//   analyzeHeadless.bat %USERPROFILE%\pd-ghidra-proj PDUWP -process PDUWP.exe -noanalysis -readOnly
//     -scriptPath engine\re -postScript DecompAt.java <out.txt> <hexaddr> [<hexaddr> ...]
//@category PD
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import java.io.PrintWriter;

public class DecompAt extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        DecompInterface di = new DecompInterface();
        di.openProgram(currentProgram);
        try (PrintWriter out = new PrintWriter(args[0], "UTF-8")) {
            for (int i = 1; i < args.length; i++) {
                Address a = toAddr(Long.parseUnsignedLong(args[i], 16));
                if (getInstructionAt(a) == null) {
                    new DisassembleCommand(a, null, true).applyTo(currentProgram);
                }
                Function f = getFunctionAt(a);
                if (f == null) {
                    f = createFunction(a, null);
                }
                out.println("//==== " + args[i]);
                if (f == null) {
                    out.println("could not create a function here");
                    continue;
                }
                DecompileResults r = di.decompileFunction(f, 120, monitor);
                out.println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "decompile failed: " + r.getErrorMessage());
            }
        }
    }
}
