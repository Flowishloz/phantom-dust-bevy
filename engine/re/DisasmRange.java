// Print Ghidra's disassembly for address ranges (our own tool).
// Output stays outside the repo, like the decompile export.
//
// Usage (headless, read-only, no re-analysis):
//   analyzeHeadless.bat %USERPROFILE%\pd-ghidra-proj PDUWP -process PDUWP.exe -noanalysis -readOnly
//     -scriptPath engine\re -postScript DisasmRange.java <out.txt> <start>:<count> [<start>:<count> ...]
// <start> is a hex address (14026ea94), <count> the number of instructions to print.
// If the address is not yet an instruction, it is disassembled first (in memory only; -readOnly).
//@category PD
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.program.model.address.Address;
import ghidra.program.model.address.AddressSet;
import ghidra.program.model.listing.Instruction;
import java.io.PrintWriter;

public class DisasmRange extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        try (PrintWriter out = new PrintWriter(args[0], "UTF-8")) {
            for (int i = 1; i < args.length; i++) {
                String[] p = args[i].split(":");
                Address a = toAddr(Long.parseUnsignedLong(p[0], 16));
                int n = Integer.parseInt(p[1]);
                out.println("==== " + a);
                if (getInstructionAt(a) == null) {
                    new DisassembleCommand(new AddressSet(a, a.add(n * 16L)), null, true).applyTo(currentProgram);
                }
                Instruction ins = getInstructionAt(a);
                if (ins == null) {
                    // Not an instruction start (mid-instruction address): begin at the next one.
                    ins = getInstructionAfter(a);
                }
                for (int k = 0; k < n && ins != null; k++) {
                    out.println(ins.getAddress() + "  " + ins);
                    ins = ins.getNext();
                }
            }
        }
    }
}
