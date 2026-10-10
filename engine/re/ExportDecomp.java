// Headless Ghidra script (our own code): decompile every function in the
// current program and write them to one text file, plus a function list.
// Output folder = first script argument (default %USERPROFILE%/.pdtools/re-out).
// Usage (see docs/PROGRAM-RE.md):
//   analyzeHeadless <proj> PDUWP -process PDUWP.exe -noanalysis -readOnly
//     -scriptPath engine/re -postScript ExportDecomp.java <outdir>
// The output is decompiled game code: it stays outside the repo.
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import java.io.*;

public class ExportDecomp extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        String out = args.length > 0 ? args[0] : System.getenv("USERPROFILE") + "/.pdtools/re-out";
        DecompInterface d = new DecompInterface();
        d.openProgram(currentProgram);
        FunctionIterator it = currentProgram.getFunctionManager().getFunctions(true);
        int n = 0, fail = 0;
        try (PrintWriter w = new PrintWriter(new BufferedWriter(new FileWriter(out + "/decomp_all.c")));
             PrintWriter l = new PrintWriter(new FileWriter(out + "/functions.txt"))) {
            while (it.hasNext() && !monitor.isCancelled()) {
                Function f = it.next();
                l.println(f.getEntryPoint() + " " + f.getName() + " " + f.getBody().getNumAddresses());
                DecompileResults r = d.decompileFunction(f, 60, monitor);
                w.println("//==== " + f.getEntryPoint() + " " + f.getName());
                if (r != null && r.decompileCompleted()) {
                    w.println(r.getDecompiledFunction().getC());
                } else {
                    fail++;
                    w.println("// decompile failed");
                }
                n++;
                if (n % 1000 == 0) println("decompiled " + n);
            }
        }
        println("done " + n + " functions, " + fail + " failed");
    }
}
