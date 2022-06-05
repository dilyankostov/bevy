//! Everything in this module is pending to be refactored, turned into idiomatic-rust, and moved to
//! other modules.

//! The contents of this file are a combination of transpilation and human
//! modification to Morten S. Mikkelsen's original tangent space algorithm
//! implementation written in C. The original source code can be found at
//! <https://archive.blender.org/wiki/index.php/Dev:Shading/Tangent_Space_Normal_Maps>
//! and includes the following licence:
//!
//! Copyright (C) 2011 by Morten S. Mikkelsen
//!
//! This software is provided 'as-is', without any express or implied
//! warranty.  In no event will the authors be held liable for any damages
//! arising from the use of this software.
//!
//! Permission is granted to anyone to use this software for any purpose,
//! including commercial applications, and to alter it and redistribute it
//! freely, subject to the following restrictions:
//!
//! 1. The origin of this software must not be misrepresented; you must not
//! claim that you wrote the original software. If you use this software
//! in a product, an acknowledgment in the product documentation would be
//! appreciated but is not required.
//!
//! 2. Altered source versions must be plainly marked as such, and must not be
//! misrepresented as being the original software.
//!
//! 3. This notice may not be removed or altered from any source distribution.
//!
// Comments starting with `C:` are copied as-is from the original
// Note that some comments may originate from the original but not be marked as such

#![allow(
    clippy::all,
    clippy::doc_markdown,
    clippy::redundant_else,
    clippy::match_same_arms,
    clippy::semicolon_if_nothing_returned,
    clippy::explicit_iter_loop,
    clippy::map_flatten,
    dead_code,
    mutable_transmutes,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unused_mut,
    unused_assignments,
    unused_variables
)]

use std::ptr::null_mut;

use bitflags::bitflags;
use glam::Vec3;

use crate::{face_vert_to_index, get_normal, get_position, get_tex_coord, FaceKind, Geometry};

#[derive(Copy, Clone)]
pub struct STSpace {
    // Normalised f
    pub vOs: Vec3,
    pub fMagS: f32,
    pub vOt: Vec3,
    pub fMagT: f32,
    pub iCounter: i32,
    pub bOrient: bool,
}

impl STSpace {
    pub fn zero() -> Self {
        Self {
            vOs: Default::default(),
            fMagS: 0.0,
            vOt: Default::default(),
            fMagT: 0.0,
            iCounter: 0,
            bOrient: false,
        }
    }
}

// To avoid visual errors (distortions/unwanted hard edges in lighting), when using sampled normal maps, the
// normal map sampler must use the exact inverse of the pixel shader transformation.
// The most efficient transformation we can possibly do in the pixel shader is
// achieved by using, directly, the "unnormalized" interpolated tangent, bitangent and vertex normal: vT, vB and vN.
// pixel shader (fast transform out)
// vNout = normalize( vNt.x * vT + vNt.y * vB + vNt.z * vN );
// where vNt is the tangent space normal. The normal map sampler must likewise use the
// interpolated and "unnormalized" tangent, bitangent and vertex normal to be compliant with the pixel shader.
// sampler does (exact inverse of pixel shader):
// float3 row0 = cross(vB, vN);
// float3 row1 = cross(vN, vT);
// float3 row2 = cross(vT, vB);
// float fSign = dot(vT, row0)<0 ? -1 : 1;
// vNt = normalize( fSign * float3(dot(vNout,row0), dot(vNout,row1), dot(vNout,row2)) );
// where vNout is the sampled normal in some chosen 3D space.
//
// Should you choose to reconstruct the bitangent in the pixel shader instead
// of the vertex shader, as explained earlier, then be sure to do this in the normal map sampler also.
// Finally, beware of quad triangulations. If the normal map sampler doesn't use the same triangulation of
// quads as your renderer then problems will occur since the interpolated tangent spaces will differ
// eventhough the vertex level tangent spaces match. This can be solved either by triangulating before
// sampling/exporting or by using the order-independent choice of diagonal for splitting quads suggested earlier.
// However, this must be used both by the sampler and your tools/rendering pipeline.
// internal structure

bitflags! {
    pub struct TriangleFlags: u8 {
        const DEGENERATE = 1;
        const QUAD_ONE_DEGENERATE_TRI = 2;
        const GROUP_WITH_ANY = 4;
        const ORIENT_PRESERVING = 8;
    }
}

#[derive(Copy, Clone)]
pub struct STriInfo {
    pub FaceNeighbors: [i32; 3],
    pub AssignedGroup: [*mut SGroup; 3],
    pub vOs: Vec3,
    pub vOt: Vec3,
    pub fMagS: f32,
    pub fMagT: f32,
    pub iOrgFaceNumber: i32,
    pub iFlag: TriangleFlags,
    pub iTSpacesOffs: i32,
    pub vert_num: [u8; 4],
}

impl STriInfo {
    fn zero() -> Self {
        Self {
            FaceNeighbors: [0, 0, 0],
            AssignedGroup: [null_mut(), null_mut(), null_mut()],
            vOs: Default::default(),
            vOt: Default::default(),
            fMagS: 0.0,
            fMagT: 0.0,
            iOrgFaceNumber: 0,
            iFlag: TriangleFlags::empty(),
            iTSpacesOffs: 0,
            vert_num: [0, 0, 0, 0],
        }
    }
}

#[derive(Copy, Clone)]
pub struct SGroup {
    pub iNrFaces: i32,
    pub pFaceIndices: *mut i32,
    pub iVertexRepresentitive: i32,
    pub bOrientPreservering: bool,
}

impl SGroup {
    fn zero() -> Self {
        Self {
            iNrFaces: 0,
            pFaceIndices: null_mut(),
            iVertexRepresentitive: 0,
            bOrientPreservering: false,
        }
    }
}

#[derive(Clone)]
pub struct SSubGroup {
    pub iNrFaces: i32,
    pub pTriMembers: Vec<i32>,
}

impl SSubGroup {
    fn zero() -> Self {
        Self {
            iNrFaces: 0,
            pTriMembers: Vec::new(),
        }
    }
}

#[derive(Copy, Clone)]
pub union SEdge {
    pub unnamed: unnamed,
    pub array: [i32; 3],
}

impl SEdge {
    fn zero() -> Self {
        Self { array: [0, 0, 0] }
    }
}

#[derive(Copy, Clone)]
pub struct unnamed {
    pub i0: i32,
    pub i1: i32,
    pub f: i32,
}

#[derive(Copy, Clone)]
pub struct STmpVert {
    pub vert: [f32; 3],
    pub index: i32,
}

impl STmpVert {
    fn zero() -> Self {
        Self {
            vert: [0.0, 0.0, 0.0],
            index: 0,
        }
    }
}

pub unsafe fn genTangSpace(geometry: &mut impl Geometry, fAngularThreshold: f32) -> bool {
    let iNrFaces = geometry.num_faces();
    // TODO: Accept in radians by default here?
    let fThresCos = (fAngularThreshold.to_radians()).cos();

    let mut iNrTrianglesIn = 0;
    for f in 0..iNrFaces {
        let verts = geometry.num_vertices_of_face(f);
        match verts {
            FaceKind::Triangle => iNrTrianglesIn += 1,
            FaceKind::Quad => iNrTrianglesIn += 2,
        }
    }

    if iNrTrianglesIn <= 0 {
        // Easier if we can assume there's at least one face later
        // No tangents need to be generated
        return false;
    }
    let iNrTrianglesIn = iNrTrianglesIn;
    let mut piTriListIn = vec![0i32; 3 * iNrTrianglesIn];
    let mut pTriInfos = vec![STriInfo::zero(); iNrTrianglesIn];

    // C: Make an initial triangle --> face index list
    // This also handles quads
    // TODO: Make this return triangle_info and tri_face_map
    // probably in a single structure.
    let iNrTSPaces = GenerateInitialVerticesIndexList(
        &mut pTriInfos,
        &mut piTriListIn,
        geometry,
        iNrTrianglesIn,
    );
    // C: Make a welded index list of identical positions and attributes (pos, norm, texc)
    GenerateSharedVerticesIndexList(piTriListIn.as_mut_ptr(), geometry, iNrTrianglesIn);
    let iTotTris = iNrTrianglesIn;
    let mut iDegenTriangles = 0;
    // C: Mark all degenerate triangles
    for t in 0..(iTotTris as usize) {
        let i0 = piTriListIn[t * 3 + 0];
        let i1 = piTriListIn[t * 3 + 1];
        let i2 = piTriListIn[t * 3 + 2];
        let p0 = get_position(geometry, i0 as usize);
        let p1 = get_position(geometry, i1 as usize);
        let p2 = get_position(geometry, i2 as usize);
        if p0 == p1 || p0 == p2 || p1 == p2 {
            pTriInfos[t].iFlag.insert(TriangleFlags::DEGENERATE);
            iDegenTriangles += 1
        }
    }
    let iNrTrianglesIn = iTotTris - iDegenTriangles;
    // C: Mark all triangle pairs that belong to a quad with only one
    // C: good triangle. These need special treatment in DegenEpilogue().
    // C: Additionally, move all good triangles to the start of
    // C: pTriInfos[] and piTriListIn[] without changing order and
    // C: put the degenerate triangles last.
    // Note: A quad can have degenerate traignles if two vertices are in the same location
    DegenPrologue(
        pTriInfos.as_mut_ptr(),
        piTriListIn.as_mut_ptr(),
        iNrTrianglesIn as i32,
        iTotTris as i32,
    );
    // C: Evaluate triangle level attributes and neighbor list
    InitTriInfo(
        pTriInfos.as_mut_ptr(),
        piTriListIn.as_ptr(),
        geometry,
        iNrTrianglesIn,
    );
    //C: Based on the 4 rules, identify groups based on connectivity
    let iNrMaxGroups = iNrTrianglesIn * 3;

    let mut pGroups = vec![SGroup::zero(); iNrMaxGroups];
    let mut piGroupTrianglesBuffer = vec![0; iNrTrianglesIn * 3];

    let iNrActiveGroups = Build4RuleGroups(
        pTriInfos.as_mut_ptr(),
        pGroups.as_mut_ptr(),
        piGroupTrianglesBuffer.as_mut_ptr(),
        piTriListIn.as_ptr(),
        iNrTrianglesIn as i32,
    );

    let mut psTspace = vec![
        STSpace {
            vOs: Vec3::new(1.0, 0.0, 0.0),
            fMagS: 1.0,
            vOt: Vec3::new(0.0, 1.0, 0.0),
            fMagT: 1.0,
            ..STSpace::zero()
        };
        iNrTSPaces
    ];

    let bRes = GenerateTSpaces(
        &mut psTspace,
        pTriInfos.as_ptr(),
        pGroups.as_ptr(),
        iNrActiveGroups,
        piTriListIn.as_ptr(),
        fThresCos,
        geometry,
    );
    if !bRes {
        return false;
    }
    DegenEpilogue(
        psTspace.as_mut_ptr(),
        pTriInfos.as_mut_ptr(),
        piTriListIn.as_mut_ptr(),
        geometry,
        iNrTrianglesIn as i32,
        iTotTris as i32,
    );
    let mut index = 0;
    for f in 0..iNrFaces {
        let verts_0 = geometry.num_vertices_of_face(f);
        for i in 0..verts_0.num_vertices() {
            let mut pTSpace: *const STSpace = &mut psTspace[index] as *mut STSpace;
            let mut tang = Vec3::new((*pTSpace).vOs.x, (*pTSpace).vOs.y, (*pTSpace).vOs.z);
            let mut bitang = Vec3::new((*pTSpace).vOt.x, (*pTSpace).vOt.y, (*pTSpace).vOt.z);
            geometry.set_tangent(
                tang.into(),
                bitang.into(),
                (*pTSpace).fMagS,
                (*pTSpace).fMagT,
                (*pTSpace).bOrient,
                f,
                i,
            );
            index += 1;
        }
    }

    return true;
}
unsafe fn DegenEpilogue(
    mut psTspace: *mut STSpace,
    mut pTriInfos: *mut STriInfo,
    mut piTriListIn: *mut i32,
    geometry: &impl Geometry,
    iNrTrianglesIn: i32,
    iTotTris: i32,
) {
    // For all degenerate triangles
    for t in iNrTrianglesIn..iTotTris {
        let bSkip: bool = (*pTriInfos.offset(t as isize))
            .iFlag
            .contains(TriangleFlags::QUAD_ONE_DEGENERATE_TRI);
        if !bSkip {
            for i in 0..3i32 {
                // For all vertices on that triangle
                let index1: i32 = *piTriListIn.offset((t * 3i32 + i) as isize);
                for j in 0..(3i32 * iNrTrianglesIn) {
                    let index2: i32 = *piTriListIn.offset(j as isize);
                    // If the vertex properties are the same as another non-degenerate vertex
                    if index1 == index2 {
                        let iTri: i32 = j / 3i32;
                        let iVert: i32 = j % 3i32;
                        let iSrcVert: i32 =
                            (*pTriInfos.offset(iTri as isize)).vert_num[iVert as usize] as i32;
                        let iSrcOffs: i32 = (*pTriInfos.offset(iTri as isize)).iTSpacesOffs;
                        let iDstVert: i32 =
                            (*pTriInfos.offset(t as isize)).vert_num[i as usize] as i32;
                        let iDstOffs: i32 = (*pTriInfos.offset(t as isize)).iTSpacesOffs;
                        // Set the tangent space of this vertex to the tangent space of that vertex
                        // TODO: This is absurd - doing a linear search through all vertices for each
                        // degenerate triangle?
                        *psTspace.offset((iDstOffs + iDstVert) as isize) =
                            *psTspace.offset((iSrcOffs + iSrcVert) as isize);
                        break;
                    }
                }
            }
        }
    }
    for t in 0..iNrTrianglesIn {
        // Handle quads with a single degenerate triangle by
        if (*pTriInfos.offset(t as isize))
            .iFlag
            .contains(TriangleFlags::QUAD_ONE_DEGENERATE_TRI)
        {
            let mut pV: *mut u8 = (*pTriInfos.offset(t as isize)).vert_num.as_mut_ptr();
            let mut iFlag: i32 = 1i32 << *pV.offset(0isize) as i32
                | 1i32 << *pV.offset(1isize) as i32
                | 1i32 << *pV.offset(2isize) as i32;
            let mut iMissingIndex: i32 = 0i32;
            if iFlag & 2i32 == 0i32 {
                iMissingIndex = 1i32
            } else if iFlag & 4i32 == 0i32 {
                iMissingIndex = 2i32
            } else if iFlag & 8i32 == 0i32 {
                iMissingIndex = 3i32
            }
            let iOrgF = (*pTriInfos.offset(t as isize)).iOrgFaceNumber;
            let vDstP = get_position(
                geometry,
                face_vert_to_index(iOrgF as usize, iMissingIndex as usize),
            );

            for i_0 in 0..3i32 {
                let iVert_0: i32 = *pV.offset(i_0 as isize) as i32;
                let vSrcP = get_position(
                    geometry,
                    face_vert_to_index(iOrgF as usize, iVert_0 as usize),
                );
                if vSrcP == vDstP {
                    let iOffs: i32 = (*pTriInfos.offset(t as isize)).iTSpacesOffs;
                    *psTspace.offset((iOffs + iMissingIndex) as isize) =
                        *psTspace.offset((iOffs + iVert_0) as isize);
                    break;
                }
            }
        }
    }
}

unsafe fn GenerateTSpaces(
    psTspace: &mut [STSpace],
    mut pTriInfos: *const STriInfo,
    mut pGroups: *const SGroup,
    iNrActiveGroups: i32,
    mut piTriListIn: *const i32,
    fThresCos: f32,
    geometry: &impl Geometry,
) -> bool {
    let mut iMaxNrFaces: usize = 0;
    let mut iUniqueTspaces = 0;
    for g in 0..iNrActiveGroups {
        if iMaxNrFaces < (*pGroups.offset(g as isize)).iNrFaces as usize {
            iMaxNrFaces = (*pGroups.offset(g as isize)).iNrFaces as usize
        }
    }
    if iMaxNrFaces == 0 {
        return true;
    }

    let mut pSubGroupTspace = vec![STSpace::zero(); iMaxNrFaces];
    let mut pUniSubGroups = vec![SSubGroup::zero(); iMaxNrFaces];
    let mut pTmpMembers = vec![0i32; iMaxNrFaces];

    iUniqueTspaces = 0;

    for g in 0..iNrActiveGroups {
        let mut pGroup: *const SGroup = &*pGroups.offset(g as isize) as *const SGroup;
        let mut iUniqueSubGroups = 0;
        let mut s = 0;

        for i in 0..(*pGroup).iNrFaces {
            let f: i32 = *(*pGroup).pFaceIndices.offset(i as isize);
            let mut index: i32 = -1i32;
            let mut tmp_group: SSubGroup = SSubGroup {
                iNrFaces: 0,
                pTriMembers: Vec::new(),
            };
            let mut vOs = Vec3::new(0.0, 0.0, 0.0);
            let mut vOt = Vec3::new(0.0, 0.0, 0.0);
            if (*pTriInfos.offset(f as isize)).AssignedGroup[0usize] == pGroup as *mut SGroup {
                index = 0i32
            } else if (*pTriInfos.offset(f as isize)).AssignedGroup[1usize] == pGroup as *mut SGroup
            {
                index = 1i32
            } else if (*pTriInfos.offset(f as isize)).AssignedGroup[2usize] == pGroup as *mut SGroup
            {
                index = 2i32
            }
            let iVertIndex = *piTriListIn.offset((f * 3i32 + index) as isize);
            let n = get_normal(geometry, iVertIndex as usize);
            let mut vOs = (*pTriInfos.offset(f as isize)).vOs
                - (n.dot((*pTriInfos.offset(f as isize)).vOs) * n);
            let mut vOt = (*pTriInfos.offset(f as isize)).vOt
                - (n.dot((*pTriInfos.offset(f as isize)).vOt) * n);
            if VNotZero(vOs) {
                vOs = Normalize(vOs)
            }
            if VNotZero(vOt) {
                vOt = Normalize(vOt)
            }
            let iOF_1 = (*pTriInfos.offset(f as isize)).iOrgFaceNumber;
            let mut iMembers = 0;

            for j in 0..(*pGroup).iNrFaces {
                let t: i32 = *(*pGroup).pFaceIndices.offset(j as isize);
                let iOF_2: i32 = (*pTriInfos.offset(t as isize)).iOrgFaceNumber;
                let mut vOs2 = (*pTriInfos.offset(t as isize)).vOs
                    - (n.dot((*pTriInfos.offset(t as isize)).vOs) * n);
                let mut vOt2 = (*pTriInfos.offset(t as isize)).vOt
                    - (n.dot((*pTriInfos.offset(t as isize)).vOt) * n);
                if VNotZero(vOs2) {
                    vOs2 = Normalize(vOs2)
                }
                if VNotZero(vOt2) {
                    vOt2 = Normalize(vOt2)
                }
                let bAny: bool = ((*pTriInfos.offset(f as isize)).iFlag
                    | (*pTriInfos.offset(t as isize)).iFlag)
                    .contains(TriangleFlags::GROUP_WITH_ANY);
                let bSameOrgFace: bool = iOF_1 == iOF_2;
                let fCosS: f32 = vOs.dot(vOs2);
                let fCosT: f32 = vOt.dot(vOt2);
                if bAny || bSameOrgFace || fCosS > fThresCos && fCosT > fThresCos {
                    let fresh0 = iMembers;
                    iMembers = iMembers + 1;
                    pTmpMembers[fresh0] = t
                }
            }
            if iMembers > 1 {
                pTmpMembers[0..(iMembers - 1)].sort();
            }
            tmp_group.iNrFaces = iMembers as i32;
            tmp_group.pTriMembers = pTmpMembers.clone();

            let mut found = None;
            for l in 0..iUniqueSubGroups {
                if tmp_group == pUniSubGroups[l] {
                    found = Some(l);
                    break;
                }
            }
            let idx;
            if let Some(it) = found {
                idx = it;
            } else {
                idx = iUniqueSubGroups;
                // C: if no match was found we allocate a new subgroup
                pUniSubGroups[iUniqueSubGroups].iNrFaces = iMembers as i32;
                pUniSubGroups[iUniqueSubGroups].pTriMembers = tmp_group.pTriMembers.clone();

                pSubGroupTspace[iUniqueSubGroups] = EvalTspace(
                    tmp_group.pTriMembers.as_mut_ptr(),
                    iMembers as i32,
                    piTriListIn,
                    pTriInfos,
                    geometry,
                    (*pGroup).iVertexRepresentitive,
                );
                iUniqueSubGroups += 1
            }
            let iOffs = (*pTriInfos.offset(f as isize)).iTSpacesOffs as usize;
            let iVert = (*pTriInfos.offset(f as isize)).vert_num[index as usize] as usize;
            let mut pTS_out: *mut STSpace = &mut psTspace[iOffs + iVert] as *mut STSpace;
            if (*pTS_out).iCounter == 1i32 {
                *pTS_out = AvgTSpace(pTS_out, &mut pSubGroupTspace[idx]);
                (*pTS_out).iCounter = 2i32;
                (*pTS_out).bOrient = (*pGroup).bOrientPreservering
            } else {
                *pTS_out = pSubGroupTspace[idx];
                (*pTS_out).iCounter = 1i32;
                (*pTS_out).bOrient = (*pGroup).bOrientPreservering
            }
        }
        iUniqueTspaces += iUniqueSubGroups;
    }
    return true;
}
unsafe fn AvgTSpace(mut pTS0: *const STSpace, mut pTS1: *const STSpace) -> STSpace {
    let mut ts_res: STSpace = STSpace {
        vOs: Vec3::new(0.0, 0.0, 0.0),
        fMagS: 0.,
        vOt: Vec3::new(0.0, 0.0, 0.0),
        fMagT: 0.,
        iCounter: 0,
        bOrient: false,
    };
    if (*pTS0).fMagS == (*pTS1).fMagS
        && (*pTS0).fMagT == (*pTS1).fMagT
        && (*pTS0).vOs == (*pTS1).vOs
        && (*pTS0).vOt == (*pTS1).vOt
    {
        ts_res.fMagS = (*pTS0).fMagS;
        ts_res.fMagT = (*pTS0).fMagT;
        ts_res.vOs = (*pTS0).vOs;
        ts_res.vOt = (*pTS0).vOt
    } else {
        ts_res.fMagS = 0.5f32 * ((*pTS0).fMagS + (*pTS1).fMagS);
        ts_res.fMagT = 0.5f32 * ((*pTS0).fMagT + (*pTS1).fMagT);
        ts_res.vOs = (*pTS0).vOs + (*pTS1).vOs;
        ts_res.vOt = (*pTS0).vOt + (*pTS1).vOt;
        if VNotZero(ts_res.vOs) {
            ts_res.vOs = Normalize(ts_res.vOs)
        }
        if VNotZero(ts_res.vOt) {
            ts_res.vOt = Normalize(ts_res.vOt)
        }
    }
    return ts_res;
}

unsafe fn Normalize(v: Vec3) -> Vec3 {
    return (1.0 / v.length()) * v;
}

unsafe fn VNotZero(v: Vec3) -> bool {
    NotZero(v.x) || NotZero(v.y) || NotZero(v.z)
}

unsafe fn NotZero(fX: f32) -> bool {
    fX.abs() > 1.17549435e-38f32
}

unsafe fn EvalTspace(
    mut face_indices: *mut i32,
    iFaces: i32,
    mut piTriListIn: *const i32,
    mut pTriInfos: *const STriInfo,
    geometry: &impl Geometry,
    iVertexRepresentitive: i32,
) -> STSpace {
    let mut res: STSpace = STSpace::zero();
    let mut fAngleSum: f32 = 0i32 as f32;

    for face in 0..iFaces {
        let f: i32 = *face_indices.offset(face as isize);
        if !(*pTriInfos.offset(f as isize))
            .iFlag
            .contains(TriangleFlags::GROUP_WITH_ANY)
        {
            let mut vOs = Vec3::new(0.0, 0.0, 0.0);
            let mut vOt = Vec3::new(0.0, 0.0, 0.0);

            let mut fCos: f32 = 0.;
            let mut i: i32 = -1i32;

            if *piTriListIn.offset((3i32 * f + 0i32) as isize) == iVertexRepresentitive {
                i = 0i32
            } else if *piTriListIn.offset((3i32 * f + 1i32) as isize) == iVertexRepresentitive {
                i = 1i32
            } else if *piTriListIn.offset((3i32 * f + 2i32) as isize) == iVertexRepresentitive {
                i = 2i32
            }
            let index = *piTriListIn.offset((3i32 * f + i) as isize);
            let n = get_normal(geometry, index as usize);
            let mut vOs = (*pTriInfos.offset(f as isize)).vOs
                - (n.dot((*pTriInfos.offset(f as isize)).vOs) * n);
            let mut vOt = (*pTriInfos.offset(f as isize)).vOt
                - (n.dot((*pTriInfos.offset(f as isize)).vOt) * n);
            if VNotZero(vOs) {
                vOs = Normalize(vOs)
            }
            if VNotZero(vOt) {
                vOt = Normalize(vOt)
            }
            let i2 =
                *piTriListIn.offset((3i32 * f + if i < 2i32 { i + 1i32 } else { 0i32 }) as isize);
            let i1 = *piTriListIn.offset((3i32 * f + i) as isize);
            let i0 =
                *piTriListIn.offset((3i32 * f + if i > 0i32 { i - 1i32 } else { 2i32 }) as isize);
            let p0 = get_position(geometry, i0 as usize);
            let p1 = get_position(geometry, i1 as usize);
            let p2 = get_position(geometry, i2 as usize);
            let v1 = p0 - p1;
            let v2 = p2 - p1;
            let mut v1 = v1 - (n.dot(v1) * n);
            if VNotZero(v1) {
                v1 = Normalize(v1)
            }
            let mut v2 = v2 - (n.dot(v2) * n);
            if VNotZero(v2) {
                v2 = Normalize(v2)
            }
            let fCos = v1.dot(v2);

            let fCos = if fCos > 1i32 as f32 {
                1i32 as f32
            } else if fCos < -1i32 as f32 {
                -1i32 as f32
            } else {
                fCos
            };
            let fAngle = (fCos as f64).acos() as f32;
            let fMagS = (*pTriInfos.offset(f as isize)).fMagS;
            let fMagT = (*pTriInfos.offset(f as isize)).fMagT;
            res.vOs = res.vOs + (fAngle * vOs);
            res.vOt = res.vOt + (fAngle * vOt);
            res.fMagS += fAngle * fMagS;
            res.fMagT += fAngle * fMagT;
            fAngleSum += fAngle
        }
    }
    if VNotZero(res.vOs) {
        res.vOs = Normalize(res.vOs)
    }
    if VNotZero(res.vOt) {
        res.vOt = Normalize(res.vOt)
    }
    if fAngleSum > 0i32 as f32 {
        res.fMagS /= fAngleSum;
        res.fMagT /= fAngleSum
    }
    return res;
}

unsafe fn Build4RuleGroups(
    mut pTriInfos: *mut STriInfo,
    mut pGroups: *mut SGroup,
    mut piGroupTrianglesBuffer: *mut i32,
    mut piTriListIn: *const i32,
    iNrTrianglesIn: i32,
) -> i32 {
    let iNrMaxGroups: i32 = iNrTrianglesIn * 3i32;
    let mut iNrActiveGroups: i32 = 0i32;
    let mut iOffset: i32 = 0i32;

    for f in 0..iNrTrianglesIn {
        for i in 0..3i32 {
            if !(*pTriInfos.offset(f as isize))
                .iFlag
                .contains(TriangleFlags::GROUP_WITH_ANY)
                && (*pTriInfos.offset(f as isize)).AssignedGroup[i as usize].is_null()
            {
                let vert_index: i32 = *piTriListIn.offset((f * 3i32 + i) as isize);
                let ref mut fresh2 = (*pTriInfos.offset(f as isize)).AssignedGroup[i as usize];
                *fresh2 = &mut *pGroups.offset(iNrActiveGroups as isize) as *mut SGroup;
                (*(*pTriInfos.offset(f as isize)).AssignedGroup[i as usize])
                    .iVertexRepresentitive = vert_index;
                (*(*pTriInfos.offset(f as isize)).AssignedGroup[i as usize]).bOrientPreservering =
                    (*pTriInfos.offset(f as isize))
                        .iFlag
                        .contains(TriangleFlags::ORIENT_PRESERVING);
                (*(*pTriInfos.offset(f as isize)).AssignedGroup[i as usize]).iNrFaces = 0i32;
                let ref mut fresh3 =
                    (*(*pTriInfos.offset(f as isize)).AssignedGroup[i as usize]).pFaceIndices;
                *fresh3 = &mut *piGroupTrianglesBuffer.offset(iOffset as isize) as *mut i32;
                iNrActiveGroups += 1;
                AddTriToGroup((*pTriInfos.offset(f as isize)).AssignedGroup[i as usize], f);
                let bOrPre = (*pTriInfos.offset(f as isize))
                    .iFlag
                    .contains(TriangleFlags::ORIENT_PRESERVING);
                let mut neigh_indexL = (*pTriInfos.offset(f as isize)).FaceNeighbors[i as usize];
                let mut neigh_indexR = (*pTriInfos.offset(f as isize)).FaceNeighbors
                    [(if i > 0i32 { i - 1i32 } else { 2i32 }) as usize];
                if neigh_indexL >= 0i32 {
                    let bAnswer: bool = AssignRecur(
                        piTriListIn,
                        pTriInfos,
                        neigh_indexL,
                        (*pTriInfos.offset(f as isize)).AssignedGroup[i as usize],
                    );
                    let bOrPre2: bool = (*pTriInfos.offset(neigh_indexL as isize))
                        .iFlag
                        .contains(TriangleFlags::ORIENT_PRESERVING);
                    let bDiff: bool = if bOrPre != bOrPre2 { true } else { false };
                }
                if neigh_indexR >= 0i32 {
                    let bAnswer_0: bool = AssignRecur(
                        piTriListIn,
                        pTriInfos,
                        neigh_indexR,
                        (*pTriInfos.offset(f as isize)).AssignedGroup[i as usize],
                    );
                    let bOrPre2_0: bool = (*pTriInfos.offset(neigh_indexR as isize))
                        .iFlag
                        .contains(TriangleFlags::ORIENT_PRESERVING);
                    let bDiff_0: bool = if bOrPre != bOrPre2_0 { true } else { false };
                }
                iOffset += (*(*pTriInfos.offset(f as isize)).AssignedGroup[i as usize]).iNrFaces
            }
        }
    }
    return iNrActiveGroups;
}
// ///////////////////////////////////////////////////////////////////////////////////////////////////
/////////////////////////////////////////////////////////////////////////////////////////////////////
unsafe fn AssignRecur(
    mut piTriListIn: *const i32,
    mut psTriInfos: *mut STriInfo,
    iMyTriIndex: i32,
    mut pGroup: *mut SGroup,
) -> bool {
    let mut pMyTriInfo: *mut STriInfo =
        &mut *psTriInfos.offset(iMyTriIndex as isize) as *mut STriInfo;
    // track down vertex
    let iVertRep: i32 = (*pGroup).iVertexRepresentitive;
    let mut pVerts: *const i32 =
        &*piTriListIn.offset((3i32 * iMyTriIndex + 0i32) as isize) as *const i32;
    let mut i: i32 = -1i32;
    if *pVerts.offset(0isize) == iVertRep {
        i = 0i32
    } else if *pVerts.offset(1isize) == iVertRep {
        i = 1i32
    } else if *pVerts.offset(2isize) == iVertRep {
        i = 2i32
    }
    if (*pMyTriInfo).AssignedGroup[i as usize] == pGroup {
        return true;
    } else {
        if !(*pMyTriInfo).AssignedGroup[i as usize].is_null() {
            return false;
        }
    }
    if (*pMyTriInfo).iFlag.contains(TriangleFlags::GROUP_WITH_ANY) {
        if (*pMyTriInfo).AssignedGroup[0usize].is_null()
            && (*pMyTriInfo).AssignedGroup[1usize].is_null()
            && (*pMyTriInfo).AssignedGroup[2usize].is_null()
        {
            (*pMyTriInfo).iFlag.set(
                TriangleFlags::ORIENT_PRESERVING,
                (*pGroup).bOrientPreservering,
            );
        }
    }
    let bOrient: bool = (*pMyTriInfo)
        .iFlag
        .contains(TriangleFlags::ORIENT_PRESERVING);
    if bOrient != (*pGroup).bOrientPreservering {
        return false;
    }
    AddTriToGroup(pGroup, iMyTriIndex);
    (*pMyTriInfo).AssignedGroup[i as usize] = pGroup;
    let neigh_indexL: i32 = (*pMyTriInfo).FaceNeighbors[i as usize];
    let neigh_indexR: i32 =
        (*pMyTriInfo).FaceNeighbors[(if i > 0i32 { i - 1i32 } else { 2i32 }) as usize];
    if neigh_indexL >= 0i32 {
        AssignRecur(piTriListIn, psTriInfos, neigh_indexL, pGroup);
    }
    if neigh_indexR >= 0i32 {
        AssignRecur(piTriListIn, psTriInfos, neigh_indexR, pGroup);
    }
    return true;
}
unsafe fn AddTriToGroup(mut pGroup: *mut SGroup, iTriIndex: i32) {
    *(*pGroup).pFaceIndices.offset((*pGroup).iNrFaces as isize) = iTriIndex;
    (*pGroup).iNrFaces += 1;
}
unsafe fn InitTriInfo(
    mut pTriInfos: *mut STriInfo,
    mut piTriListIn: *const i32,
    geometry: &impl Geometry,
    iNrTrianglesIn: usize,
) {
    for f in 0..iNrTrianglesIn {
        for i in 0..3i32 {
            (*pTriInfos.offset(f as isize)).FaceNeighbors[i as usize] = -1i32;
            let ref mut fresh4 = (*pTriInfos.offset(f as isize)).AssignedGroup[i as usize];
            *fresh4 = 0 as *mut SGroup;
            (*pTriInfos.offset(f as isize)).vOs.x = 0.0f32;
            (*pTriInfos.offset(f as isize)).vOs.y = 0.0f32;
            (*pTriInfos.offset(f as isize)).vOs.z = 0.0f32;
            (*pTriInfos.offset(f as isize)).vOt.x = 0.0f32;
            (*pTriInfos.offset(f as isize)).vOt.y = 0.0f32;
            (*pTriInfos.offset(f as isize)).vOt.z = 0.0f32;
            (*pTriInfos.offset(f as isize)).fMagS = 0i32 as f32;
            (*pTriInfos.offset(f as isize)).fMagT = 0i32 as f32;
            (*pTriInfos.offset(f as isize))
                .iFlag
                .insert(TriangleFlags::GROUP_WITH_ANY);
        }
    }
    for f in 0..iNrTrianglesIn {
        let v1 = get_position(geometry, *piTriListIn.offset((f * 3 + 0) as isize) as usize);
        let v2 = get_position(geometry, *piTriListIn.offset((f * 3 + 1) as isize) as usize);
        let v3 = get_position(geometry, *piTriListIn.offset((f * 3 + 2) as isize) as usize);
        let t1 = get_tex_coord(geometry, *piTriListIn.offset((f * 3 + 0) as isize) as usize);
        let t2 = get_tex_coord(geometry, *piTriListIn.offset((f * 3 + 1) as isize) as usize);
        let t3 = get_tex_coord(geometry, *piTriListIn.offset((f * 3 + 2) as isize) as usize);
        let t21x: f32 = t2.x - t1.x;
        let t21y: f32 = t2.y - t1.y;
        let t31x: f32 = t3.x - t1.x;
        let t31y: f32 = t3.y - t1.y;
        let d1 = v2 - v1;
        let d2 = v3 - v1;
        let fSignedAreaSTx2: f32 = t21x * t31y - t21y * t31x;
        let mut vOs = (t31y * d1) - (t21y * d2);
        let mut vOt = (-t31x * d1) + (t21x * d2);
        if fSignedAreaSTx2 > 0.0 {
            (*pTriInfos.offset(f as isize))
                .iFlag
                .insert(TriangleFlags::ORIENT_PRESERVING);
        }
        if NotZero(fSignedAreaSTx2) {
            let fAbsArea: f32 = fSignedAreaSTx2.abs();
            let fLenOs: f32 = vOs.length();
            let fLenOt: f32 = vOt.length();
            let fS: f32 = if !(*pTriInfos.offset(f as isize))
                .iFlag
                .contains(TriangleFlags::ORIENT_PRESERVING)
            {
                -1.0f32
            } else {
                1.0f32
            };
            if NotZero(fLenOs) {
                (*pTriInfos.offset(f as isize)).vOs = (fS / fLenOs) * vOs
            }
            if NotZero(fLenOt) {
                (*pTriInfos.offset(f as isize)).vOt = (fS / fLenOt) * vOt
            }
            (*pTriInfos.offset(f as isize)).fMagS = fLenOs / fAbsArea;
            (*pTriInfos.offset(f as isize)).fMagT = fLenOt / fAbsArea;
            if NotZero((*pTriInfos.offset(f as isize)).fMagS)
                && NotZero((*pTriInfos.offset(f as isize)).fMagT)
            {
                (*pTriInfos.offset(f as isize))
                    .iFlag
                    .remove(TriangleFlags::GROUP_WITH_ANY);
            }
        }
    }
    let mut t = 0;
    while t < iNrTrianglesIn - 1 {
        let iFO_a: i32 = (*pTriInfos.offset(t as isize)).iOrgFaceNumber;
        let iFO_b: i32 = (*pTriInfos.offset((t + 1) as isize)).iOrgFaceNumber;
        if iFO_a == iFO_b {
            let bIsDeg_a: bool = (*pTriInfos.offset(t as isize))
                .iFlag
                .contains(TriangleFlags::DEGENERATE);
            let bIsDeg_b: bool = (*pTriInfos.offset((t + 1) as isize))
                .iFlag
                .contains(TriangleFlags::DEGENERATE);
            if !(bIsDeg_a || bIsDeg_b) {
                let bOrientA: bool = (*pTriInfos.offset(t as isize))
                    .iFlag
                    .contains(TriangleFlags::ORIENT_PRESERVING);
                let bOrientB: bool = (*pTriInfos.offset((t + 1) as isize))
                    .iFlag
                    .contains(TriangleFlags::ORIENT_PRESERVING);
                if bOrientA != bOrientB {
                    let mut bChooseOrientFirstTri: bool = false;
                    if (*pTriInfos.offset((t + 1) as isize))
                        .iFlag
                        .contains(TriangleFlags::GROUP_WITH_ANY)
                    {
                        bChooseOrientFirstTri = true
                    } else if CalcTexArea(geometry, &*piTriListIn.offset((t * 3 + 0) as isize))
                        >= CalcTexArea(geometry, &*piTriListIn.offset(((t + 1) * 3 + 0) as isize))
                    {
                        bChooseOrientFirstTri = true
                    }
                    let t0 = if bChooseOrientFirstTri { t } else { t + 1 };
                    let t1_0 = if bChooseOrientFirstTri { t + 1 } else { t };
                    (*pTriInfos.offset(t1_0 as isize)).iFlag.set(
                        TriangleFlags::ORIENT_PRESERVING,
                        (*pTriInfos.offset(t0 as isize))
                            .iFlag
                            .contains(TriangleFlags::ORIENT_PRESERVING),
                    );
                }
            }
            t += 2
        } else {
            t += 1
        }
    }

    let mut pEdges = vec![SEdge::zero(); iNrTrianglesIn * 3];
    BuildNeighborsFast(pTriInfos, &mut pEdges, piTriListIn, iNrTrianglesIn as i32);
}

unsafe fn BuildNeighborsFast(
    mut pTriInfos: *mut STriInfo,
    mut pEdges: &mut [SEdge],
    mut piTriListIn: *const i32,
    iNrTrianglesIn: i32,
) {
    // build array of edges
    // could replace with a random seed?
    let mut uSeed: u32 = 39871946i32 as u32;
    let mut iEntries: i32 = 0i32;
    let mut iCurStartIndex: i32 = -1i32;

    for f in 0..iNrTrianglesIn {
        for i in 0..3i32 {
            let i0: i32 = *piTriListIn.offset((f * 3i32 + i) as isize);
            let i1: i32 =
                *piTriListIn.offset((f * 3i32 + if i < 2i32 { i + 1i32 } else { 0i32 }) as isize);
            pEdges[((f * 3i32 + i) as usize)].i0 = if i0 < i1 { i0 } else { i1 }; // put minimum index in i0
            (pEdges[(f * 3i32 + i) as usize]).i1 = if !(i0 < i1) { i0 } else { i1 }; // put maximum index in i1
            (pEdges[(f * 3i32 + i) as usize]).f = f; // record face number
        }
    }
    pEdges.sort();

    for i in 0..iEntries {
        let edge = pEdges[i as usize];
        let i0_0: i32 = edge.i0;
        let i1_0: i32 = edge.i1;
        let f_0: i32 = edge.f;

        let mut i0_A: i32 = 0;
        let mut i1_A: i32 = 0;
        let mut edgenum_A: i32 = 0;
        let mut edgenum_B: i32 = 0i32;
        GetEdge(
            &mut i0_A,
            &mut i1_A,
            &mut edgenum_A,
            &*piTriListIn.offset((f_0 * 3i32) as isize),
            i0_0,
            i1_0,
        );
        let bUnassigned_A =
            (*pTriInfos.offset(f_0 as isize)).FaceNeighbors[edgenum_A as usize] == -1i32;
        if bUnassigned_A {
            let mut j: i32 = i + 1i32;

            let mut bNotFound: bool = true;
            while j < iEntries
                && i0_0 == pEdges[j as usize].i0
                && i1_0 == pEdges[j as usize].i1
                && bNotFound
            {
                let mut i0_B: i32 = 0;
                let mut i1_B: i32 = 0;
                let t = pEdges[j as usize].f;
                GetEdge(
                    &mut i1_B,
                    &mut i0_B,
                    &mut edgenum_B,
                    &*piTriListIn.offset((t * 3i32) as isize),
                    pEdges[j as usize].i0,
                    pEdges[j as usize].i1,
                );
                let bUnassigned_B =
                    if (*pTriInfos.offset(t as isize)).FaceNeighbors[edgenum_B as usize] == -1i32 {
                        true
                    } else {
                        false
                    };
                if i0_A == i0_B && i1_A == i1_B && bUnassigned_B {
                    bNotFound = false
                } else {
                    j += 1
                }
            }
            if !bNotFound {
                let mut t_0: i32 = pEdges[j as usize].f;
                (*pTriInfos.offset(f_0 as isize)).FaceNeighbors[edgenum_A as usize] = t_0;
                (*pTriInfos.offset(t_0 as isize)).FaceNeighbors[edgenum_B as usize] = f_0
            }
        }
    }
}
unsafe fn GetEdge(
    mut i0_out: *mut i32,
    mut i1_out: *mut i32,
    mut edgenum_out: *mut i32,
    mut indices: *const i32,
    i0_in: i32,
    i1_in: i32,
) {
    *edgenum_out = -1i32;
    if *indices.offset(0isize) == i0_in || *indices.offset(0isize) == i1_in {
        if *indices.offset(1isize) == i0_in || *indices.offset(1isize) == i1_in {
            *edgenum_out.offset(0isize) = 0i32;
            *i0_out.offset(0isize) = *indices.offset(0isize);
            *i1_out.offset(0isize) = *indices.offset(1isize)
        } else {
            *edgenum_out.offset(0isize) = 2i32;
            *i0_out.offset(0isize) = *indices.offset(2isize);
            *i1_out.offset(0isize) = *indices.offset(0isize)
        }
    } else {
        *edgenum_out.offset(0isize) = 1i32;
        *i0_out.offset(0isize) = *indices.offset(1isize);
        *i1_out.offset(0isize) = *indices.offset(2isize)
    };
}
// ///////////////////////////////////////////////////////////////////////////////////////////
/////////////////////////////////////////////////////////////////////////////////////////////

// returns the texture area times 2
unsafe fn CalcTexArea(geometry: &impl Geometry, mut indices: *const i32) -> f32 {
    let t1 = get_tex_coord(geometry, *indices.offset(0isize) as usize);
    let t2 = get_tex_coord(geometry, *indices.offset(1isize) as usize);
    let t3 = get_tex_coord(geometry, *indices.offset(2isize) as usize);
    let t21x: f32 = t2.x - t1.x;
    let t21y: f32 = t2.y - t1.y;
    let t31x: f32 = t3.x - t1.x;
    let t31y: f32 = t3.y - t1.y;
    let fSignedAreaSTx2: f32 = t21x * t31y - t21y * t31x;
    return if fSignedAreaSTx2 < 0i32 as f32 {
        -fSignedAreaSTx2
    } else {
        fSignedAreaSTx2
    };
}

// degen triangles
unsafe fn DegenPrologue(
    mut pTriInfos: *mut STriInfo,
    mut piTriList_out: *mut i32,
    iNrTrianglesIn: i32,
    iTotTris: i32,
) {
    // locate quads with only one good triangle
    let mut t: i32 = 0i32;
    while t < iTotTris - 1i32 {
        let iFO_a: i32 = (*pTriInfos.offset(t as isize)).iOrgFaceNumber;
        let iFO_b: i32 = (*pTriInfos.offset((t + 1i32) as isize)).iOrgFaceNumber;
        if iFO_a == iFO_b {
            let bIsDeg_a: bool = (*pTriInfos.offset(t as isize))
                .iFlag
                .contains(TriangleFlags::DEGENERATE);
            let bIsDeg_b: bool = (*pTriInfos.offset((t + 1i32) as isize))
                .iFlag
                .contains(TriangleFlags::DEGENERATE);
            // If exactly one is degenerate, mark both as QUAD_ONE_DEGENERATE_TRI, i.e. that the other triangle
            // (If both are degenerate, this)
            if bIsDeg_a ^ bIsDeg_b {
                (*pTriInfos.offset(t as isize))
                    .iFlag
                    .insert(TriangleFlags::QUAD_ONE_DEGENERATE_TRI);
                (*pTriInfos.offset((t + 1i32) as isize))
                    .iFlag
                    .insert(TriangleFlags::QUAD_ONE_DEGENERATE_TRI);
            }
            t += 2i32
        } else {
            t += 1
        }
    }

    // reorder list so all degen triangles are moved to the back
    // without reordering the good triangles
    // That is, a semi-stable partition, e.g. as described at
    // https://dlang.org/library/std/algorithm/sorting/partition.html
    // TODO: Use `Vec::retain` with a second vec here - not perfect,
    // but good enough and safe.
    // TODO: Consider using `sort_by_key` on Vec instead (which is stable) - it might be
    // technically slower, but it's much easier to reason about
    let mut iNextGoodTriangleSearchIndex = 1i32;
    t = 0i32;
    let mut bStillFindingGoodOnes = true;
    while t < iNrTrianglesIn && bStillFindingGoodOnes {
        let bIsGood: bool = !(*pTriInfos.offset(t as isize))
            .iFlag
            .contains(TriangleFlags::DEGENERATE);
        if bIsGood {
            if iNextGoodTriangleSearchIndex < t + 2i32 {
                iNextGoodTriangleSearchIndex = t + 2i32
            }
        } else {
            let mut bJustADegenerate: bool = true;
            while bJustADegenerate && iNextGoodTriangleSearchIndex < iTotTris {
                let bIsGood_0: bool = !(*pTriInfos.offset(iNextGoodTriangleSearchIndex as isize))
                    .iFlag
                    .contains(TriangleFlags::DEGENERATE);
                if bIsGood_0 {
                    bJustADegenerate = false
                } else {
                    iNextGoodTriangleSearchIndex += 1
                }
            }
            let t0 = t;
            let t1 = iNextGoodTriangleSearchIndex;
            iNextGoodTriangleSearchIndex += 1;
            // Swap t0 and t1
            if !bJustADegenerate {
                for i in 0..3i32 {
                    let index: i32 = *piTriList_out.offset((t0 * 3i32 + i) as isize);
                    *piTriList_out.offset((t0 * 3i32 + i) as isize) =
                        *piTriList_out.offset((t1 * 3i32 + i) as isize);
                    *piTriList_out.offset((t1 * 3i32 + i) as isize) = index;
                }
                let tri_info: STriInfo = *pTriInfos.offset(t0 as isize);
                *pTriInfos.offset(t0 as isize) = *pTriInfos.offset(t1 as isize);
                *pTriInfos.offset(t1 as isize) = tri_info
            } else {
                bStillFindingGoodOnes = false
            }
        }
        if bStillFindingGoodOnes {
            t += 1
        }
    }
}
unsafe fn GenerateSharedVerticesIndexList(
    // The input vertex index->face/vert mappings
    // Identical face/verts will have each vertex index
    // point to the same (arbitrary?) face/vert
    // TODO: This seems overly complicated - storing vertex properties in a
    // side channel seems much easier.
    // Hopefully implementation can be changed to just use a btreemap or
    // something too.
    mut piTriList_in_and_out: *mut i32,
    geometry: &impl Geometry,
    iNrTrianglesIn: usize,
) {
    let mut i = 0;
    let mut iChannel: i32 = 0i32;
    let mut k = 0;
    let mut e = 0;
    let mut iMaxCount = 0;
    let mut vMin = get_position(geometry, 0);
    let mut vMax = vMin;
    let mut vDim = Vec3::new(0.0, 0.0, 0.0);
    let mut fMin: f32 = 0.;
    let mut fMax: f32 = 0.;
    i = 1;
    while i < iNrTrianglesIn * 3 {
        let index: i32 = *piTriList_in_and_out.offset(i as isize);
        let vP = get_position(geometry, index as usize);
        if vMin.x > vP.x {
            vMin.x = vP.x
        } else if vMax.x < vP.x {
            vMax.x = vP.x
        }
        if vMin.y > vP.y {
            vMin.y = vP.y
        } else if vMax.y < vP.y {
            vMax.y = vP.y
        }
        if vMin.z > vP.z {
            vMin.z = vP.z
        } else if vMax.z < vP.z {
            vMax.z = vP.z
        }
        i += 1
    }
    vDim = vMax - vMin;
    iChannel = 0i32;
    fMin = vMin.x;
    fMax = vMax.x;
    if vDim.y > vDim.x && vDim.y > vDim.z {
        iChannel = 1i32;
        fMin = vMin.y;
        fMax = vMax.y
    } else if vDim.z > vDim.x {
        iChannel = 2i32;
        fMin = vMin.z;
        fMax = vMax.z
    }

    let mut piHashTable = vec![0i32; iNrTrianglesIn * 3];
    let mut piHashOffsets = vec![0i32; g_iCells];
    let mut piHashCount = vec![0i32; g_iCells];
    let mut piHashCount2 = vec![0i32; g_iCells];

    i = 0;
    while i < iNrTrianglesIn * 3 {
        let index_0: i32 = *piTriList_in_and_out.offset(i as isize);
        let vP_0 = get_position(geometry, index_0 as usize);
        let fVal: f32 = if iChannel == 0i32 {
            vP_0.x
        } else if iChannel == 1i32 {
            vP_0.y
        } else {
            vP_0.z
        };
        let iCell = FindGridCell(fMin, fMax, fVal);
        piHashCount[iCell] += 1;
        i += 1
    }
    piHashOffsets[0] = 0i32;
    k = 1;
    while k < g_iCells {
        piHashOffsets[k] = piHashOffsets[k - 1] + piHashCount[k - 1];
        k += 1
    }
    i = 0;
    while i < iNrTrianglesIn * 3 {
        let index_1: i32 = *piTriList_in_and_out.offset(i as isize);
        let vP_1 = get_position(geometry, index_1 as usize);
        let fVal_0: f32 = if iChannel == 0i32 {
            vP_1.x
        } else if iChannel == 1i32 {
            vP_1.y
        } else {
            vP_1.z
        };
        let iCell_0 = FindGridCell(fMin, fMax, fVal_0);
        let mut pTable: *mut i32 = 0 as *mut i32;
        pTable = &mut piHashTable[piHashOffsets[iCell_0] as usize] as *mut i32;
        *pTable.offset(piHashCount2[iCell_0] as isize) = i as i32;
        piHashCount2[iCell_0] += 1;
        i += 1
    }
    k = 0;
    while k < g_iCells {
        k += 1
    }
    iMaxCount = piHashCount[0] as usize;
    k = 1;
    while k < g_iCells {
        if iMaxCount < piHashCount[k] as usize {
            iMaxCount = piHashCount[k] as usize
        }
        k += 1
    }
    let mut pTmpVert = vec![STmpVert::zero(); iMaxCount];
    k = 0;
    while k < g_iCells {
        // extract table of cell k and amount of entries in it
        let mut pTable_0 = &mut piHashTable[piHashOffsets[k] as usize] as *mut i32;
        let iEntries = piHashCount[k] as usize;
        if !(iEntries < 2) {
            e = 0;
            while e < iEntries {
                let mut i_0: i32 = *pTable_0.offset(e as isize);
                let vP_2 = get_position(
                    geometry,
                    *piTriList_in_and_out.offset(i_0 as isize) as usize,
                );
                pTmpVert[e].vert[0usize] = vP_2.x;
                pTmpVert[e].vert[1usize] = vP_2.y;
                pTmpVert[e].vert[2usize] = vP_2.z;
                pTmpVert[e].index = i_0;
                e += 1
            }
            MergeVertsFast(
                piTriList_in_and_out,
                pTmpVert.as_mut_ptr(),
                geometry,
                0i32,
                (iEntries - 1) as i32,
            );
        }
        k += 1
    }
}

unsafe fn MergeVertsFast(
    mut piTriList_in_and_out: *mut i32,
    mut pTmpVert: *mut STmpVert,
    geometry: &impl Geometry,
    iL_in: i32,
    iR_in: i32,
) {
    // make bbox
    let mut c: i32 = 0i32;
    let mut l: i32 = 0i32;
    let mut channel: i32 = 0i32;
    let mut fvMin: [f32; 3] = [0.; 3];
    let mut fvMax: [f32; 3] = [0.; 3];
    let mut dx: f32 = 0i32 as f32;
    let mut dy: f32 = 0i32 as f32;
    let mut dz: f32 = 0i32 as f32;
    let mut fSep: f32 = 0i32 as f32;
    c = 0i32;
    while c < 3i32 {
        fvMin[c as usize] = (*pTmpVert.offset(iL_in as isize)).vert[c as usize];
        fvMax[c as usize] = fvMin[c as usize];
        c += 1
    }
    l = iL_in + 1i32;
    while l <= iR_in {
        c = 0i32;
        while c < 3i32 {
            if fvMin[c as usize] > (*pTmpVert.offset(l as isize)).vert[c as usize] {
                fvMin[c as usize] = (*pTmpVert.offset(l as isize)).vert[c as usize]
            } else if fvMax[c as usize] < (*pTmpVert.offset(l as isize)).vert[c as usize] {
                fvMax[c as usize] = (*pTmpVert.offset(l as isize)).vert[c as usize]
            }
            c += 1
        }
        l += 1
    }
    dx = fvMax[0usize] - fvMin[0usize];
    dy = fvMax[1usize] - fvMin[1usize];
    dz = fvMax[2usize] - fvMin[2usize];
    channel = 0i32;
    if dy > dx && dy > dz {
        channel = 1i32
    } else if dz > dx {
        channel = 2i32
    }
    fSep = 0.5f32 * (fvMax[channel as usize] + fvMin[channel as usize]);
    if fSep >= fvMax[channel as usize] || fSep <= fvMin[channel as usize] {
        l = iL_in;
        while l <= iR_in {
            let mut i: i32 = (*pTmpVert.offset(l as isize)).index;
            let index: i32 = *piTriList_in_and_out.offset(i as isize);
            let vP = get_position(geometry, index as usize);
            let vN = get_normal(geometry, index as usize);
            let vT = get_tex_coord(geometry, index as usize);
            let mut bNotFound: bool = true;
            let mut l2: i32 = iL_in;
            let mut i2rec: i32 = -1i32;
            while l2 < l && bNotFound {
                let i2: i32 = (*pTmpVert.offset(l2 as isize)).index;
                let index2: i32 = *piTriList_in_and_out.offset(i2 as isize);
                let vP2 = get_position(geometry, index2 as usize);
                let vN2 = get_normal(geometry, index2 as usize);
                let vT2 = get_tex_coord(geometry, index2 as usize);
                i2rec = i2;
                if vP.x == vP2.x
                    && vP.y == vP2.y
                    && vP.z == vP2.z
                    && vN.x == vN2.x
                    && vN.y == vN2.y
                    && vN.z == vN2.z
                    && vT.x == vT2.x
                    && vT.y == vT2.y
                    && vT.z == vT2.z
                {
                    bNotFound = false
                } else {
                    l2 += 1
                }
            }
            if !bNotFound {
                *piTriList_in_and_out.offset(i as isize) =
                    *piTriList_in_and_out.offset(i2rec as isize)
            }
            l += 1
        }
    } else {
        let mut iL: i32 = iL_in;
        let mut iR: i32 = iR_in;
        while iL < iR {
            let mut bReadyLeftSwap: bool = false;
            let mut bReadyRightSwap: bool = false;
            while !bReadyLeftSwap && iL < iR {
                bReadyLeftSwap = !((*pTmpVert.offset(iL as isize)).vert[channel as usize] < fSep);
                if !bReadyLeftSwap {
                    iL += 1
                }
            }
            while !bReadyRightSwap && iL < iR {
                bReadyRightSwap = (*pTmpVert.offset(iR as isize)).vert[channel as usize] < fSep;
                if !bReadyRightSwap {
                    iR -= 1
                }
            }
            if bReadyLeftSwap && bReadyRightSwap {
                let sTmp: STmpVert = *pTmpVert.offset(iL as isize);
                *pTmpVert.offset(iL as isize) = *pTmpVert.offset(iR as isize);
                *pTmpVert.offset(iR as isize) = sTmp;
                iL += 1;
                iR -= 1
            }
        }
        if iL == iR {
            let bReadyRightSwap_0: bool =
                (*pTmpVert.offset(iR as isize)).vert[channel as usize] < fSep;
            if bReadyRightSwap_0 {
                iL += 1
            } else {
                iR -= 1
            }
        }
        if iL_in < iR {
            MergeVertsFast(piTriList_in_and_out, pTmpVert, geometry, iL_in, iR);
        }
        if iL < iR_in {
            MergeVertsFast(piTriList_in_and_out, pTmpVert, geometry, iL, iR_in);
        }
    };
}

const g_iCells: usize = 2048;

// it is IMPORTANT that this function is called to evaluate the hash since
// inlining could potentially reorder instructions and generate different
// results for the same effective input value fVal.
#[inline(never)]
unsafe fn FindGridCell(fMin: f32, fMax: f32, fVal: f32) -> usize {
    let fIndex = g_iCells as f32 * ((fVal - fMin) / (fMax - fMin));
    let iIndex = fIndex as isize;
    return if iIndex < g_iCells as isize {
        if iIndex >= 0 {
            iIndex as usize
        } else {
            0
        }
    } else {
        g_iCells - 1
    };
}

unsafe fn GenerateInitialVerticesIndexList(
    pTriInfos: &mut [STriInfo],
    piTriList_out: &mut [i32],
    geometry: &mut impl Geometry,
    iNrTrianglesIn: usize,
) -> usize {
    let mut iTSpacesOffs: usize = 0;
    let mut f = 0;
    let mut t: usize = 0;
    let mut iDstTriIndex = 0;
    f = 0;
    while f < geometry.num_faces() {
        let verts = geometry.num_vertices_of_face(f);

        pTriInfos[iDstTriIndex].iOrgFaceNumber = f as i32;
        pTriInfos[iDstTriIndex].iTSpacesOffs = iTSpacesOffs as i32;
        if let FaceKind::Triangle = verts {
            let mut pVerts = &mut pTriInfos[iDstTriIndex].vert_num;
            pVerts[0] = 0;
            pVerts[1] = 1;
            pVerts[2] = 2;
            piTriList_out[iDstTriIndex * 3 + 0] = face_vert_to_index(f, 0) as i32;
            piTriList_out[iDstTriIndex * 3 + 1] = face_vert_to_index(f, 1) as i32;
            piTriList_out[iDstTriIndex * 3 + 2] = face_vert_to_index(f, 2) as i32;
            iDstTriIndex += 1
        } else {
            pTriInfos[iDstTriIndex + 1].iOrgFaceNumber = f as i32;
            pTriInfos[iDstTriIndex + 1].iTSpacesOffs = iTSpacesOffs as i32;
            let i0 = face_vert_to_index(f, 0);
            let i1 = face_vert_to_index(f, 1);
            let i2 = face_vert_to_index(f, 2);
            let i3 = face_vert_to_index(f, 3);
            let T0 = get_tex_coord(geometry, i0);
            let T1 = get_tex_coord(geometry, i1);
            let T2 = get_tex_coord(geometry, i2);
            let T3 = get_tex_coord(geometry, i3);
            let distSQ_02: f32 = (T2 - T0).length_squared();
            let distSQ_13: f32 = (T3 - T1).length_squared();
            let mut bQuadDiagIs_02: bool = false;
            if distSQ_02 < distSQ_13 {
                bQuadDiagIs_02 = true
            } else if distSQ_13 < distSQ_02 {
                bQuadDiagIs_02 = false
            } else {
                let P0 = get_position(geometry, i0);
                let P1 = get_position(geometry, i1);
                let P2 = get_position(geometry, i2);
                let P3 = get_position(geometry, i3);
                let distSQ_02_0: f32 = (P2 - P0).length_squared();
                let distSQ_13_0: f32 = (P3 - P1).length_squared();
                bQuadDiagIs_02 = if distSQ_13_0 < distSQ_02_0 {
                    false
                } else {
                    true
                }
            }
            if bQuadDiagIs_02 {
                let mut pVerts_A = &mut pTriInfos[iDstTriIndex].vert_num;
                pVerts_A[0] = 0;
                pVerts_A[1] = 1;
                pVerts_A[2] = 2;
                piTriList_out[iDstTriIndex * 3 + 0] = i0 as i32;
                piTriList_out[iDstTriIndex * 3 + 1] = i1 as i32;
                piTriList_out[iDstTriIndex * 3 + 2] = i2 as i32;
                iDstTriIndex += 1;

                let mut pVerts_B = &mut pTriInfos[iDstTriIndex].vert_num;
                pVerts_B[0] = 0;
                pVerts_B[1] = 2;
                pVerts_B[2] = 3;
                piTriList_out[iDstTriIndex * 3 + 0] = i0 as i32;
                piTriList_out[iDstTriIndex * 3 + 1] = i2 as i32;
                piTriList_out[iDstTriIndex * 3 + 2] = i3 as i32;
                iDstTriIndex += 1
            } else {
                let mut pVerts_A_0 = &mut pTriInfos[iDstTriIndex].vert_num;
                pVerts_A_0[0] = 0;
                pVerts_A_0[1] = 1;
                pVerts_A_0[2] = 3;
                piTriList_out[iDstTriIndex * 3 + 0] = i0 as i32;
                piTriList_out[iDstTriIndex * 3 + 1] = i1 as i32;
                piTriList_out[iDstTriIndex * 3 + 2] = i3 as i32;
                iDstTriIndex += 1;

                let mut pVerts_B_0 = &mut pTriInfos[iDstTriIndex].vert_num;
                pVerts_B_0[0] = 1;
                pVerts_B_0[1] = 2;
                pVerts_B_0[2] = 3;
                piTriList_out[iDstTriIndex * 3 + 0] = i1 as i32;
                piTriList_out[iDstTriIndex * 3 + 1] = i2 as i32;
                piTriList_out[iDstTriIndex * 3 + 2] = i3 as i32;
                iDstTriIndex += 1
            }
        }
        iTSpacesOffs += verts.num_vertices();

        f += 1
    }
    t = 0;
    while t < iNrTrianglesIn {
        pTriInfos[t].iFlag = TriangleFlags::empty();
        t += 1
    }
    return iTSpacesOffs;
}
