// S33D's exception-safe C boundary to OpenCascade. MPL-2.0.
#include <Standard_Version.hxx>
#include <Standard_Failure.hxx>
#include <STEPControl_Reader.hxx>
#include <STEPControl_Writer.hxx>
#include <IGESControl_Reader.hxx>
#include <IGESControl_Writer.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepAlgoAPI_Check.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepBuilderAPI_GTransform.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <GeomConvert_SurfToAnaSurf.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <BRepBuilderAPI_NurbsConvert.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <gp_Pln.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepLib.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Geom_BezierSurface.hxx>
#include <GeomAPI_PointsToBSplineSurface.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <Geom_BSplineCurve.hxx>
#include <TColgp_HArray1OfPnt.hxx>
#include <TColStd_HArray1OfReal.hxx>
#include <Geom_BSplineSurface.hxx>
#include <TColgp_Array2OfPnt.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepGProp.hxx>
#include <BRepBndLib.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_ListOfShape.hxx>
#include <GeomLProp_SLProps.hxx>
#include <gp_GTrsf.hxx>
#include <gp_Trsf.hxx>
#include <cmath>
#include <cstdint>
#include <string>
#include <vector>
#include <sstream>
#include <memory>
#include <stdexcept>
#include <chrono>
#include <cstdlib>
#include <cstdio>
#include <algorithm>
#include <map>
#include <numeric>

namespace {
struct StageTimer {
  const char* label;
  std::chrono::steady_clock::time_point start=std::chrono::steady_clock::now();
  explicit StageTimer(const char* text):label(text) {}
  ~StageTimer() {
    if(std::getenv("S33D_GEOMETRY_TIMINGS")) {
      std::fprintf(stderr,"[geometry] %s %.3fs\n",label,std::chrono::duration<double>(std::chrono::steady_clock::now()-start).count());
      std::fflush(stderr);
    }
  }
};
struct ShapeData {
  std::string brep;
  std::vector<float> positions, normals;
  std::vector<uint32_t> indices, faces;
  std::vector<uint8_t> planar_faces;
  uint32_t face_count = 0, solid_count = 0;
  double volume = 0;
};
struct Result {
  std::string error, text;
  std::vector<ShapeData> shapes;
};
void require(bool ok, const char* message) { if (!ok) throw std::runtime_error(message); }
TopoDS_Shape read_brep(const char* text) {
  require(text && *text, "Empty B-Rep payload");
  std::istringstream stream(text);
  TopoDS_Shape shape;
  BRep_Builder builder;
  BRepTools::Read(shape, stream, builder);
  require(!shape.IsNull(), "Cannot read B-Rep payload");
  return shape;
}
ShapeData data_for(const TopoDS_Shape& original, bool validated=false) {
  StageTimer timer("serialize/mesh");
  require(!original.IsNull(), "Operation returned an empty shape");
  if(!validated) require(BRepCheck_Analyzer(original).IsValid(), "Invalid B-Rep topology");
  ShapeData data;
  std::ostringstream stream;
  BRepTools::Write(original, stream, false, false, TopTools_FormatVersion_CURRENT);
  data.brep = stream.str();
  // Tessellate the canonical persisted topology, so triangle->face IDs agree
  // with a later operation after deserialization, including Undo and reopen.
  auto shape = read_brep(data.brep.c_str());
  TopTools_IndexedMapOfShape faces, solids;
  TopExp::MapShapes(shape, TopAbs_FACE, faces);
  TopExp::MapShapes(shape, TopAbs_SOLID, solids);
  data.face_count = faces.Extent(); data.solid_count = solids.Extent();
  require(data.face_count > 0, "Shape has no surfaces");
  if (data.solid_count) {
    GProp_GProps props; BRepGProp::VolumeProperties(shape, props);
    data.volume = props.Mass();
  }
  BRepMesh_IncrementalMesh mesher(shape, 0.05, false, 0.25, false);
  require(mesher.IsDone(), "Surface tessellation failed");
  for (int f = 1; f <= faces.Extent(); ++f) {
    const auto face = TopoDS::Face(faces(f));
    data.planar_faces.push_back(BRepAdaptor_Surface(face).GetType()==GeomAbs_Plane ? 1 : 0);
    TopLoc_Location loc, surface_loc;
    const auto tri = BRep_Tool::Triangulation(face, loc);
    require(!tri.IsNull(), "A face could not be tessellated");
    const auto surface = BRep_Tool::Surface(face, surface_loc);
    require(data.positions.size() / 3 + tri->NbNodes() < 10000000, "Display mesh exceeds safety limit");
    const auto base = static_cast<uint32_t>(data.positions.size() / 3);
    for (int i = 1; i <= tri->NbNodes(); ++i) {
      auto point = tri->Node(i).Transformed(loc.Transformation());
      data.positions.insert(data.positions.end(), {float(point.X()), float(point.Y()), float(point.Z())});
      gp_Vec normal(0,0,0);
      if (tri->HasUVNodes() && !surface.IsNull()) {
        const auto uv = tri->UVNode(i);
        GeomLProp_SLProps props(surface, uv.X(), uv.Y(), 1, 1e-9);
        if (props.IsNormalDefined()) {
          normal = gp_Vec(props.Normal()).Transformed(surface_loc.Transformation());
          if (face.Orientation() == TopAbs_REVERSED) normal.Reverse();
        }
      }
      data.normals.insert(data.normals.end(), {float(normal.X()), float(normal.Y()), float(normal.Z())});
    }
    for (int i = 1; i <= tri->NbTriangles(); ++i) {
      int a,b,c; tri->Triangle(i).Get(a,b,c);
      if (face.Orientation() == TopAbs_REVERSED) std::swap(b,c);
      data.indices.insert(data.indices.end(), {base+uint32_t(a-1),base+uint32_t(b-1),base+uint32_t(c-1)});
      data.faces.push_back(f-1);
    }
  }
  return data;
}
template<class F> Result* guarded(F function) noexcept {
  Result* result = nullptr;
  try { result = new Result; function(*result); }
  catch (const Standard_Failure& e) { if(result) { result->shapes.clear(); result->error = e.GetMessageString() ? e.GetMessageString() : "OpenCascade failure"; } }
  catch (const std::exception& e) { if(result) { result->shapes.clear(); result->error = e.what(); } }
  catch (...) { if(result) { result->shapes.clear(); result->error = "Unknown native geometry failure"; } }
  return result;
}
void validate_result(const TopoDS_Shape& shape) {
  StageTimer timer("result validation");
  require(!shape.IsNull(), "Invalid offset / boolean result");
  // Check already performs BRepCheck_Analyzer as well as self-interference.
  // Construct first, configure parallelism, THEN perform: the convenience
  // constructor runs the expensive whole-shape check serially immediately.
  BRepAlgoAPI_Check check;
  check.SetData(shape, false, true);
  check.SetRunParallel(true);
  check.Perform();
  require(!check.HasErrors() && check.IsValid(), "Result has invalid topology or self-interference");
}

// Preserve the certified indexed graph. Geometric sewing can merge two close
// thread edges and cannot reliably reconstruct collapsed f32 boolean slivers.
TopoDS_Shape faceted_shape(const double* positions, size_t vertices,
                          const uint32_t* indices, size_t count) {
  require(positions && indices && vertices>0 && count>0 && count%3==0 && count<=300000,
          "Invalid faceted solid buffers");
  std::vector<TopoDS_Vertex> points;
  for(size_t i=0;i<vertices;++i) {
    const double* p=positions+3*i;
    require(std::isfinite(p[0])&&std::isfinite(p[1])&&std::isfinite(p[2]),"Invalid mesh coordinate");
    points.push_back(BRepBuilderAPI_MakeVertex(gp_Pnt(p[0],p[1],p[2])));
  }
  struct Edge { TopoDS_Edge shape; size_t face; int balance=0, uses=0; };
  std::map<std::pair<uint32_t,uint32_t>,Edge> edges;
  std::vector<size_t> parent(count/3); std::iota(parent.begin(),parent.end(),0);
  auto root=[&](size_t i) { while(parent[i]!=i) { parent[i]=parent[parent[i]]; i=parent[i]; } return i; };
  std::vector<TopoDS_Face> faces;
  for(size_t i=0;i<count;i+=3) {
    for(int j=0;j<3;++j) require(indices[i+j]<vertices,"Invalid mesh index");
    BRepBuilderAPI_MakeWire wire;
    for(int j=0;j<3;++j) {
      auto a=indices[i+j],b=indices[i+(j+1)%3];
      require(a!=b,"Degenerate mesh edge");
      auto key=std::minmax(a,b);
      auto entry=edges.find(key);
      if(entry==edges.end()) {
        BRepBuilderAPI_MakeEdge make(points[key.first],points[key.second]);
        require(make.IsDone(),"Mesh edge is below native modeling precision");
        entry=edges.emplace(key,Edge{make.Edge(),i/3}).first;
      } else { parent[root(i/3)]=root(entry->second.face); }
      auto& edge=entry->second; ++edge.uses; edge.balance+=a<b?1:-1;
      wire.Add(a<b ? edge.shape : TopoDS::Edge(edge.shape.Reversed()));
    }
    require(wire.IsDone(),"Cannot construct mesh face wire");
    const auto a=BRep_Tool::Pnt(points[indices[i]]), b=BRep_Tool::Pnt(points[indices[i+1]]), c=BRep_Tool::Pnt(points[indices[i+2]]);
    auto normal=gp_Vec(a,b).Crossed(gp_Vec(a,c));
    require(normal.Magnitude()>1e-20,"Degenerate mesh face");
    BRepBuilderAPI_MakeFace face(gp_Pln(a,gp_Dir(normal)),wire.Wire(),true);
    require(face.IsDone(),"Cannot construct mesh planar face");
    faces.push_back(face.Face());
  }
  for(const auto& item:edges) require(item.second.uses==2 && item.second.balance==0,
                                     "Mesh is not a consistently oriented closed manifold");
  BRep_Builder builder;
  struct Component { TopoDS_Shell shell; double volume=0; gp_Pnt sample; };
  std::map<size_t,Component> components;
  // A local reference avoids cancellation for geometry far from the origin.
  const gp_Pnt origin=BRep_Tool::Pnt(points[0]);
  for(size_t f=0;f<faces.size();++f) {
    auto key=root(f); auto entry=components.find(key);
    if(entry==components.end()) {
      Component component; builder.MakeShell(component.shell);
      component.sample=BRep_Tool::Pnt(points[indices[3*f]]);
      entry=components.emplace(key,component).first;
    }
    auto& component=entry->second; builder.Add(component.shell,faces[f]);
    const gp_Vec a(origin,BRep_Tool::Pnt(points[indices[3*f]]));
    const gp_Vec b(origin,BRep_Tool::Pnt(points[indices[3*f+1]]));
    const gp_Vec c(origin,BRep_Tool::Pnt(points[indices[3*f+2]]));
    component.volume+=a.Dot(b.Crossed(c))/6.;
  }
  struct Outer { TopoDS_Solid shape; double volume; };
  std::vector<Outer> solids;
  for(auto& item:components) {
    auto& c=item.second; c.shell.Closed(true);
    require(std::abs(c.volume)>1e-18,"Mesh shell encloses no volume");
    if(c.volume>0) solids.push_back({BRepBuilderAPI_MakeSolid(c.shell).Solid(),c.volume});
  }
  require(!solids.empty(),"Mesh has inverted outer shells");
  // Negative shells are cavities, not separate filled solids. Attach each to
  // its smallest enclosing outer shell (also handles disconnected objects).
  for(const auto& item:components) {
    const auto& c=item.second; if(c.volume>0) continue;
    size_t owner=solids.size(); double volume=INFINITY;
    for(size_t i=0;i<solids.size();++i) {
      if(solids[i].volume<=-c.volume || solids[i].volume>=volume) continue;
      BRepClass3d_SolidClassifier classifier(solids[i].shape,c.sample,1e-7);
      if(classifier.State()==TopAbs_IN) { owner=i; volume=solids[i].volume; }
    }
    require(owner<solids.size(),"Mesh cavity is not enclosed by an outer shell");
    builder.Add(solids[owner].shape,c.shell);
  }
  TopoDS_Compound compound; builder.MakeCompound(compound);
  for(auto& solid:solids) {
    require(BRepCheck_Analyzer(solid.shape).IsValid(),"Faceted solid failed B-Rep validation");
    builder.Add(compound,solid.shape);
  }
  return compound;
}

std::string step_text(const TopoDS_Shape& source, bool z_up) {
  TopoDS_Shape shape=source;
  if(z_up) {
    gp_Trsf rotation; rotation.SetRotation(gp_Ax1(gp_Pnt(0,0,0),gp_Dir(1,0,0)),std::acos(-1.)/2.);
    shape=BRepBuilderAPI_Transform(shape,rotation,false).Shape();
  }
  STEPControl_Writer writer;
  require(writer.Transfer(shape, STEPControl_AsIs)==IFSelect_RetDone,"STEP transfer failed");
  std::ostringstream stream;
  require(writer.WriteStream(stream)==IFSelect_RetDone,"STEP write failed");
  return stream.str();
}
}
extern "C" const char* s33d_occt_version() noexcept { return OCC_VERSION_COMPLETE; }
extern "C" Result* s33d_occt_cylinders(const char* brep) noexcept {
  return guarded([&](Result& r) {
    StageTimer timer("cylindrical wall query");
    const bool diagnostics=std::getenv("S33D_GEOMETRY_TIMINGS")!=nullptr;
    TopTools_IndexedMapOfShape faces; TopExp::MapShapes(read_brep(brep),TopAbs_FACE,faces);
    struct Wall {
      int id; gp_Pnt base; gp_Vec axis; double radius,lo,hi; bool internal;
      TopoDS_Face face;
    };
    std::vector<Wall> walls;
    constexpr double tolerance=1e-4;
    // STEP/IGES often encode a genuine cylindrical wall as rational splines,
    // or split a full turn into several faces. Recognize without changing the
    // persisted shape or its face IDs. A group is accepted only if its actual
    // topological boundary consists of the two axial end rings.
    for(int i=1;i<=faces.Extent();++i) {
      try {
        auto face=TopoDS::Face(faces(i)); BRepAdaptor_Surface s(face);
        double u0=s.FirstUParameter(),u1=s.LastUParameter();
        double v0=s.FirstVParameter(),v1=s.LastVParameter();
        if(!std::isfinite(u0)||!std::isfinite(u1)||!std::isfinite(v0)||!std::isfinite(v1)) continue;
        gp_Cylinder c;
        if(s.GetType()==GeomAbs_Cylinder) c=s.Cylinder();
        else {
          if(s.GetType()!=GeomAbs_BSplineSurface && s.GetType()!=GeomAbs_BezierSurface &&
             s.GetType()!=GeomAbs_SurfaceOfRevolution && s.GetType()!=GeomAbs_SurfaceOfExtrusion) continue;
          auto surface=BRep_Tool::Surface(face);
          double su0,su1,sv0,sv1; surface->Bounds(su0,su1,sv0,sv1);
          GeomConvert_SurfToAnaSurf fit(surface);
          fit.SetConvType(GeomConvert_Target); fit.SetTarget(GeomAbs_Cylinder);
          // STEP trim limits can overrun a spline knot domain by roundoff.
          // SurfToAnaSurf rejects even a microscopic overrun without fitting.
          auto analytic=fit.ConvertToAnalytical(tolerance,std::max(u0,su0),std::min(u1,su1),std::max(v0,sv0),std::min(v1,sv1));
          if(analytic.IsNull()) {
            if(diagnostics) std::fprintf(stderr,"[cylinder] face %d: no analytic fit, uv %.12g %.12g %.12g %.12g / %.12g %.12g %.12g %.12g\n",i-1,u0,u1,v0,v1,su0,su1,sv0,sv1);
            continue;
          }
          GeomAdaptor_Surface adapted(analytic);
          if(adapted.GetType()!=GeomAbs_Cylinder) continue;
          c=adapted.Cylinder();
        }
        auto d=gp_Vec(c.Axis().Direction());
        // Stable axis orientation across faces with reversed parameterization.
        int k=std::abs(d.X())>=std::abs(d.Y())?1:2;
        if(std::abs(d.Z())>std::abs(d.Coord(k))) k=3;
        if(d.Coord(k)<0) d.Reverse();
        auto base=c.Location().Translated(d*(-gp_Vec(gp_Pnt(0,0,0),c.Location()).Dot(d)));
        double lo=1e100,hi=-1e100; bool valid=true,internal=false;
        for(int a=0;a<=8 && valid;++a) for(int b=0;b<=8;++b) {
          gp_Pnt p; gp_Vec du,dv;
          s.D1(u0+(u1-u0)*a/8.,v0+(v1-v0)*b/8.,p,du,dv);
          auto relative=gp_Vec(base,p); double z=relative.Dot(d);
          auto radial=relative-d*z; auto normal=du.Crossed(dv);
          if(std::abs(radial.Magnitude()-c.Radius())>tolerance ||
             normal.SquareMagnitude()<1e-20 || std::abs(normal.Normalized().Dot(d))>1e-4) {valid=false;break;}
          lo=std::min(lo,z); hi=std::max(hi,z);
          if(a==4 && b==4) {
            if(face.Orientation()==TopAbs_REVERSED) normal.Reverse();
            internal=normal.Dot(radial)<0;
          }
        }
        if(diagnostics) std::fprintf(stderr,"[cylinder] face %d: radius %.9g, range %.9g %.9g, samples %d\n",i-1,c.Radius(),lo,hi,valid?1:0);
        if(valid && hi-lo>tolerance) walls.push_back({i-1,base,d,c.Radius(),lo,hi,internal,face});
      } catch(const Standard_Failure&) {
        // A single unrecognizable spline must not hide other valid walls.
      }
    }
    std::ostringstream out; out.precision(17);
    std::vector<bool> used(walls.size(),false);
    for(size_t i=0;i<walls.size();++i) {
      if(used[i]) continue;
      const auto& w=walls[i]; std::vector<size_t> group={i}; used[i]=true;
      double lo=w.lo,hi=w.hi;
      for(size_t n=0;n<group.size();++n) for(size_t j=0;j<walls.size();++j) {
        if(used[j]) continue;
        const auto& q=walls[j];
        if(q.internal!=w.internal || std::abs(q.radius-w.radius)>tolerance ||
           q.axis.Dot(w.axis)<1.-1e-10 || q.base.Distance(w.base)>tolerance) continue;
        bool adjacent=false;
        for(TopExp_Explorer e(walls[group[n]].face,TopAbs_EDGE);e.More()&&!adjacent;e.Next())
          for(TopExp_Explorer f(q.face,TopAbs_EDGE);f.More();f.Next())
            if(e.Current().IsSame(f.Current())) {adjacent=true;break;}
        if(adjacent) {used[j]=true;group.push_back(j);lo=std::min(lo,q.lo);hi=std::max(hi,q.hi);}
      }
      TopTools_IndexedMapOfShape edges; std::vector<int> counts;
      double area=0;
      for(auto j:group) {
        // Default fixed quadrature errs by ~0.27% on rational cylinders,
        // enough to reject a valid full turn. Request adaptive integration.
        GProp_GProps props; BRepGProp::SurfaceProperties(walls[j].face,props,1e-7); area+=props.Mass();
        for(TopExp_Explorer e(walls[j].face,TopAbs_EDGE);e.More();e.Next()) {
          int index=edges.Add(e.Current()); if(size_t(index)>counts.size()) counts.resize(index,0);
          ++counts[index-1];
        }
      }
      bool valid=true,lowRing=false,highRing=false;
      for(int k=1;k<=edges.Extent() && valid;++k) {
        if(counts[k-1]==2) continue;
        if(counts[k-1]!=1) {valid=false;break;}
        auto edge=TopoDS::Edge(edges(k));
        if(BRep_Tool::Degenerated(edge)) continue;
        BRepAdaptor_Curve curve(edge); bool atLow=true,atHigh=true;
        for(int a=0;a<=8;++a) {
          auto p=curve.Value(curve.FirstParameter()+(curve.LastParameter()-curve.FirstParameter())*a/8.);
          double z=gp_Vec(w.base,p).Dot(w.axis);
          atLow=atLow && std::abs(z-lo)<tolerance;
          atHigh=atHigh && std::abs(z-hi)<tolerance;
        }
        if(!atLow && !atHigh) valid=false;
        lowRing=lowRing||atLow; highRing=highRing||atHigh;
      }
      double expected=6.283185307179586*w.radius*(hi-lo);
      if(diagnostics) std::fprintf(stderr,"[cylinder] group %d: %zu faces, boundary %d/%d/%d, area ratio %.9g\n",w.id,group.size(),valid?1:0,lowRing?1:0,highRing?1:0,area/expected);
      if(!valid || !lowRing || !highRing || std::abs(area/expected-1.)>1e-3) continue;
      auto p=w.base.Translated(w.axis*lo);
      out<<w.id<<' '<<p.X()<<' '<<p.Y()<<' '<<p.Z()<<' '<<w.axis.X()<<' '<<w.axis.Y()<<' '<<w.axis.Z()<<' '<<w.radius<<' '<<hi-lo<<' '<<(w.internal?1:0)<<'\n';
    }
    r.text=out.str();
  });
}
extern "C" Result* s33d_occt_read(const char* brep) noexcept {
  return guarded([&](Result& r) { r.shapes.push_back(data_for(read_brep(brep))); });
}
extern "C" Result* s33d_occt_mesh(const double* positions, size_t vertices, const uint32_t* indices, size_t count) noexcept {
  return guarded([&](Result& r) {
    // Modeling promotion needs complete coplanar faces: a subsequent face
    // pocket must act on a whole box side, not one display triangle. Export
    // deliberately bypasses this potentially expensive unification step.
    const auto shape=faceted_shape(positions,vertices,indices,count);
    ShapeUpgrade_UnifySameDomain unify(shape,true,true,false); unify.Build();
    validate_result(unify.Shape());
    r.shapes.push_back(data_for(unify.Shape(),true));
  });
}
extern "C" Result* s33d_occt_mesh_step(const double* positions, size_t vertices, const uint32_t* indices, size_t count) noexcept {
  return guarded([&](Result& r) { r.text=step_text(faceted_shape(positions,vertices,indices,count),true); });
}
extern "C" Result* s33d_occt_revolve(const double* points, const uint32_t* sizes, size_t loops,
 const double* origin, const double* direction, double angle) noexcept {
  return guarded([&](Result& r) {
    require(points && sizes && loops>0 && loops<=1024 && std::isfinite(angle) && std::abs(angle)>1e-9,"Invalid revolve profile");
    const gp_Ax1 axis(gp_Pnt(origin[0],origin[1],origin[2]),gp_Dir(direction[0],direction[1],direction[2]));
    BRepBuilderAPI_MakeFace face;
    size_t offset=0;
    for(size_t ring=0;ring<loops;++ring) {
      require(sizes[ring]>=3 && sizes[ring]<=100000,"Invalid revolve boundary");
      BRepBuilderAPI_MakePolygon polygon;
      for(uint32_t j=0;j<sizes[ring];++j) {
        const auto p=points+3*(offset++); gp_Pnt point(p[0],p[1],p[2]);
        // Trigonometric semicircle endpoints must lie exactly on the axis,
        // rather than sweeping a microscopic cylindrical seam at a pole.
        gp_Vec delta(axis.Location(),point); const auto along=delta.Dot(gp_Vec(axis.Direction()));
        const auto projected=axis.Location().Translated(gp_Vec(axis.Direction())*along);
        if(point.Distance(projected)<1e-8) point=projected;
        polygon.Add(point);
      }
      polygon.Close(); require(polygon.IsDone(),"Cannot build revolve boundary");
      if(ring==0) face=BRepBuilderAPI_MakeFace(polygon.Wire(),true); else face.Add(polygon.Wire());
    }
    require(face.IsDone(),"Revolve profile must be planar");
    BRepPrimAPI_MakeRevol revolve(face.Face(),axis,angle,true);
    require(revolve.IsDone(),"Revolve failed");
    auto output=revolve.Shape();
    if(output.ShapeType()==TopAbs_SOLID) {auto solid=TopoDS::Solid(output);require(BRepLib::OrientClosedSolid(solid),"Cannot orient revolved solid");output=solid;}
    validate_result(output); auto data=data_for(output,true);
    require(data.solid_count>0 && data.volume>1e-9,"Revolve did not create a closed positive-volume solid");
    r.shapes.push_back(std::move(data));
  });
}
extern "C" Result* s33d_occt_surface(const double* points, size_t nu, size_t nv, bool fill) noexcept {
  return guarded([&](Result& r) {
    require(points && nu>=4 && nv>=4 && nu<=65 && nv<=65,"Invalid surface grid");
    auto point=[&](size_t i,size_t j){const auto p=points+3*(i*nv+j);return gp_Pnt(p[0],p[1],p[2]);};
    bool periodic=true;
    for(size_t j=0;j<nv;++j) if(point(0,j).Distance(point(nu-1,j))>1e-7) periodic=false;
    const size_t rows=periodic?nu-1:nu;
    TColgp_Array2OfPnt grid(1,int(rows),1,int(nv));
    for(size_t i=0;i<rows;++i) for(size_t j=0;j<nv;++j) grid.SetValue(int(i+1),int(j+1),point(i,j));
    GeomAPI_PointsToBSplineSurface fit;
    fit.Interpolate(grid,Approx_IsoParametric,periodic);
    require(fit.IsDone(),"Surface interpolation failed; change the domain or equation");
    Handle(Geom_BSplineSurface) surface=fit.Surface();
    bool closed_v=true;
    for(size_t i=0;i<nu;++i) if(point(i,0).Distance(point(i,nv-1))>1e-7) closed_v=false;
    if(periodic && closed_v) {
      // Build a genuinely periodic tensor interpolant in BOTH directions.
      // Making a clamped V spline periodic after fitting leaves a seam kink.
      auto interpolate=[](const std::vector<gp_Pnt>& values) {
        Handle(TColgp_HArray1OfPnt) pts=new TColgp_HArray1OfPnt(1,int(values.size()));
        Handle(TColStd_HArray1OfReal) parameters=new TColStd_HArray1OfReal(1,int(values.size()+1));
        for(size_t i=0;i<values.size();++i) pts->SetValue(int(i+1),values[i]);
        for(size_t i=0;i<=values.size();++i) parameters->SetValue(int(i+1),double(i)/values.size());
        GeomAPI_Interpolate curve(pts,parameters,true,1e-9);curve.Perform();
        require(curve.IsDone(),"Periodic equation interpolation failed");return curve.Curve();
      };
      std::vector<Handle(Geom_BSplineCurve)> u_curves,v_curves;
      for(size_t j=0;j<nv-1;++j) {std::vector<gp_Pnt> values;for(size_t i=0;i<nu-1;++i) values.push_back(point(i,j));u_curves.push_back(interpolate(values));}
      for(int i=1;i<=u_curves.front()->NbPoles();++i) {std::vector<gp_Pnt> values;for(const auto& c:u_curves) values.push_back(c->Pole(i));v_curves.push_back(interpolate(values));}
      const auto& u=u_curves.front();const auto& v=v_curves.front();
      TColgp_Array2OfPnt poles(1,u->NbPoles(),1,v->NbPoles());
      for(int i=1;i<=u->NbPoles();++i) for(int j=1;j<=v->NbPoles();++j) poles.SetValue(i,j,v_curves[i-1]->Pole(j));
      surface=new Geom_BSplineSurface(poles,u->Knots(),v->Knots(),u->Multiplicities(),v->Multiplicities(),u->Degree(),v->Degree(),true,true);
    }
    BRepBuilderAPI_Sewing sewing(1e-6);
    if(periodic && closed_v) {
      // Doubly closed sheets need separate parameter patches: a single face
      // with two crossing periodic seams is ambiguous to interference checks.
      double u0,u1,v0,v1;surface->Bounds(u0,u1,v0,v1);
      for(int i=0;i<2;++i) for(int j=0;j<2;++j) {
        BRepBuilderAPI_MakeFace patch(surface,u0+(u1-u0)*i/2,u0+(u1-u0)*(i+1)/2,v0+(v1-v0)*j/2,v0+(v1-v0)*(j+1)/2,1e-7);
        require(patch.IsDone(),"Cannot build a regular equation surface patch");sewing.Add(patch.Face());
      }
    } else {
      BRepBuilderAPI_MakeFace face(surface,1e-7);
      require(face.IsDone(),"Cannot build a regular equation surface");sewing.Add(face.Face());
    }
    sewing.Perform();
    const auto sewn=sewing.SewedShape();
    TopoDS_Shell shell; BRep_Builder builder; builder.MakeShell(shell);
    for(TopExp_Explorer e(sewn,TopAbs_FACE);e.More();e.Next()) builder.Add(shell,e.Current());
    // Fill must never add an artificial cap to an open parameter boundary.
    if(fill) {
      require(sewing.NbFreeEdges()==0 && sewing.NbMultipleEdges()==0 && BRep_Tool::IsClosed(shell),
        "Fill requires an already closed surface; open surfaces are not capped");
      auto solid=BRepBuilderAPI_MakeSolid(shell).Solid();
      require(BRepLib::OrientClosedSolid(solid),"Closed surface is not orientable");
      validate_result(solid);
      auto data=data_for(solid,true); require(data.volume>1e-9,"Fill has no positive volume");
      r.shapes.push_back(std::move(data));
    } else { validate_result(shell); r.shapes.push_back(data_for(shell,true)); }
  });
}
static void imported_shapes(Result& r, TopoDS_Shape shape, bool separate, bool normalize) {
    if(normalize) {
      // Normalize the original topology BEFORE tessellation. The old path
      // serialized, validated and meshed every part twice just to place it.
      Bnd_Box bounds;
      BRepBndLib::AddOptimal(shape, bounds, false, false);
      require(!bounds.IsVoid() && !bounds.IsOpen(), "CAD model has no finite bounds");
      double xmin,ymin,zmin,xmax,ymax,zmax;
      bounds.Get(xmin,ymin,zmin,xmax,ymax,zmax);
      gp_Trsf placement;
      // STEP Z-up -> internal Y-up; preserve assembly-relative placement.
      placement.SetValues(1,0,0,-(xmin+xmax)/2, 0,0,1,-zmin, 0,-1,0,(ymin+ymax)/2);
      shape=BRepBuilderAPI_Transform(shape,placement,true).Shape();
    }
    if(!separate) { r.shapes.push_back(data_for(shape)); return; }
    TopTools_IndexedMapOfShape parts;
    TopExp::MapShapes(shape, TopAbs_SOLID, parts);
    // Keep loose shells/faces even when the same file also contains solids.
    for(TopExp_Explorer e(shape,TopAbs_SHELL,TopAbs_SOLID);e.More();e.Next()) parts.Add(e.Current());
    for(TopExp_Explorer e(shape,TopAbs_FACE,TopAbs_SHELL);e.More();e.Next()) parts.Add(e.Current());
    require(!parts.IsEmpty(), "CAD model contains no surfaces");
    for(int i=1; i<=parts.Extent(); ++i) r.shapes.push_back(data_for(parts(i)));
}
extern "C" Result* s33d_occt_import(const char* text, bool separate, bool normalize) noexcept {
  return guarded([&](Result& r) {
    StageTimer timer("STEP import");
    STEPControl_Reader reader;
    std::istringstream stream(text);
    require(reader.ReadStream("S33D.step", stream) == IFSelect_RetDone, "Cannot parse STEP");
    reader.SetSystemLengthUnit(1.0);
    require(reader.TransferRoots() > 0, "STEP contains no transferable geometry");
    imported_shapes(r, reader.OneShape(), separate, normalize);
  });
}
extern "C" Result* s33d_occt_import_iges(const char* path, bool normalize) noexcept {
  return guarded([&](Result& r) {
    StageTimer timer("IGES import");
    IGESControl_Reader reader;
    reader.SetReadVisible(false);
    require(reader.ReadFile(path) == IFSelect_RetDone, "Cannot parse IGES");
    require(reader.TransferRoots() > 0, "IGES contains no transferable geometry");
    imported_shapes(r, reader.OneShape(), true, normalize);
  });
}
extern "C" Result* s33d_occt_export_iges(const char* brep, const char* unit) noexcept {
  return guarded([&](Result& r) {
    IGESControl_Writer writer(unit, 1);
    require(writer.AddShape(read_brep(brep)), "IGES transfer failed");
    std::ostringstream stream;
    require(writer.Write(stream), "IGES write failed");
    r.text = stream.str();
  });
}
extern "C" Result* s33d_occt_export(const char* brep, bool z_up) noexcept {
  return guarded([&](Result& r) { r.text=step_text(read_brep(brep),z_up); });
}
extern "C" Result* s33d_occt_split(const char* brep, const double* origin, const double* normal) noexcept {
  return guarded([&](Result& r) {
    const auto shape=read_brep(brep);
    gp_Pnt p(origin[0],origin[1],origin[2]); gp_Dir n(normal[0],normal[1],normal[2]);
    const auto plane=BRepBuilderAPI_MakeFace(gp_Pln(p,n)).Face();
    for(double sign : {1.0,-1.0}) {
      const auto half=BRepPrimAPI_MakeHalfSpace(plane,p.Translated(gp_Vec(n)*sign)).Solid();
      BRepAlgoAPI_Common op(shape,half); require(op.IsDone(),"Exact plane split failed");
      TopTools_IndexedMapOfShape pieces;TopExp::MapShapes(op.Shape(),TopAbs_SOLID,pieces);
      for(int i=1;i<=pieces.Extent();++i) {validate_result(pieces(i));r.shapes.push_back(data_for(pieces(i),true));}
    }
    require(r.shapes.size()>=2,"Plane does not split the solid into two nonempty parts");
  });
}
extern "C" Result* s33d_occt_transform(const char* brep, const double* m) noexcept {
  return guarded([&](Result& r) {
    gp_GTrsf transform;
    for (int row=1; row<=3; ++row) for(int col=1; col<=4; ++col) transform.SetValue(row,col,m[(col-1)*4+row-1]);
    // SetForm() only classifies gp_GTrsf; it does not repair the zero scale
    // left by SetValue(). Build a fresh gp_Trsf for similarities instead.
    gp_Vec x(m[0],m[1],m[2]), y(m[4],m[5],m[6]), z(m[8],m[9],m[10]);
    const auto scale2=x.SquareMagnitude();
    const auto epsilon=scale2*1e-6;
    const bool similarity=std::abs(x.Dot(y))<epsilon && std::abs(x.Dot(z))<epsilon && std::abs(y.Dot(z))<epsilon
      && std::abs(y.SquareMagnitude()-scale2)<epsilon && std::abs(z.SquareMagnitude()-scale2)<epsilon;
    if(similarity) {
      gp_Trsf simple; simple.SetValues(m[0],m[4],m[8],m[12],m[1],m[5],m[9],m[13],m[2],m[6],m[10],m[14]);
      BRepBuilderAPI_Transform operation(read_brep(brep), simple, true);
      require(operation.IsDone(), "Exact transform failed");
      r.shapes.push_back(data_for(operation.Shape()));
    } else {
      BRepBuilderAPI_GTransform operation(read_brep(brep), transform, true);
      require(operation.IsDone(), "Exact transform failed");
      r.shapes.push_back(data_for(operation.Shape()));
    }
  });
}
extern "C" Result* s33d_occt_offset(const char* brep, double distance, double tolerance, const uint32_t* removed, size_t count, int mode) noexcept {
  return guarded([&](Result& r) {
    require(std::isfinite(distance) && std::abs(distance)>tolerance && tolerance>=1e-9 && tolerance<=1, "Invalid thickness or tolerance");
    const auto shape=read_brep(brep);
    require(BRepCheck_Analyzer(shape).IsValid(), "Input topology is invalid");
    TopoDS_Shape output;
    if(mode==3) {
      require(distance>2*tolerance,"Symmetric thickness must be positive");
      TopTools_IndexedMapOfShape solids; TopExp::MapShapes(shape,TopAbs_SOLID,solids);
      require(solids.IsEmpty(),"Symmetric thickening requires a surface, not a solid");
      auto half=[&](double amount) {
        BRepOffsetAPI_MakeThickSolid op; op.MakeThickSolidBySimple(shape,amount);
        require(op.IsDone(),"Normal thickening failed; reduce depth");
        auto result=op.Shape();
        if(result.ShapeType()==TopAbs_SHELL) {
          require(BRep_Tool::IsClosed(result),"Thickened boundary is open");
          result=BRepBuilderAPI_MakeSolid(TopoDS::Shell(result)).Solid();
        }
        require(result.ShapeType()==TopAbs_SOLID,"Thickening did not produce a solid");
        auto solid=TopoDS::Solid(result);
        require(BRepLib::OrientClosedSolid(solid),"Cannot orient thickened surface");
        validate_result(solid); return solid;
      };
      const auto positive=half(distance*0.5), negative=half(-distance*0.5);
      BRepAlgoAPI_Fuse fuse(positive,negative); require(fuse.IsDone(),"Cannot join symmetric halves");
      output=fuse.Shape();
    } else if(mode==0) {
      BRepOffsetAPI_MakeOffsetShape op;
      op.PerformByJoin(shape,distance,tolerance,BRepOffset_Skin,false,false,GeomAbs_Arc);
      require(op.IsDone(), "Surface offset failed; reduce distance or repair input");
      output=op.Shape();
    } else {
      BRepOffsetAPI_MakeThickSolid op;
      if(mode==2) {
        TopTools_IndexedMapOfShape solids; TopExp::MapShapes(shape,TopAbs_SOLID,solids);
        require(solids.IsEmpty(), "Surface thickening requires an open surface, not a solid");
        op.MakeThickSolidBySimple(shape,distance);
      } else {
        TopTools_IndexedMapOfShape faces, solids;
        TopExp::MapShapes(shape,TopAbs_FACE,faces); TopExp::MapShapes(shape,TopAbs_SOLID,solids);
        require(solids.Extent()==1, "Hollowing requires one solid");
        TopTools_ListOfShape closing;
        for(size_t i=0;i<count;++i) { require(removed[i]<uint32_t(faces.Extent()),"Selected face no longer exists"); closing.Append(faces(removed[i]+1)); }
        require(count>0, "Select at least one face to remove");
        op.MakeThickSolidByJoin(shape,closing,distance,tolerance,BRepOffset_Skin,false,false,GeomAbs_Arc);
      }
      require(op.IsDone(), "Thickening failed; reduce thickness or repair input");
      output=op.Shape();
    }
    if(mode==2) {
      // MakeThickSolidBySimple may return a closed shell or an inward-oriented
      // solid for imported open faces. Promote only genuinely closed shells.
      if(output.ShapeType()==TopAbs_SHELL) {
        require(BRep_Tool::IsClosed(output),"Thickened surface has unsealed boundaries");
        output=BRepBuilderAPI_MakeSolid(TopoDS::Shell(output)).Solid();
      }
      if(output.ShapeType()==TopAbs_SOLID) {
        auto solid=TopoDS::Solid(output);
        require(BRepLib::OrientClosedSolid(solid),"Thickened solid is not orientable");
        output=solid;
      }
    }
    validate_result(output);
    auto data=data_for(output,true);
    if(mode!=0) require(data.solid_count>0 && data.volume>0,"Result is not a closed positive-volume solid");
    r.shapes.push_back(std::move(data));
  });
}
namespace {
TopoDS_Shape face_prism(const TopoDS_Shape& source,uint32_t index,const double* normal,double depth,double start) {
    require(normal && std::isfinite(depth) && std::isfinite(start) && std::abs(depth)>1e-9,"Invalid face extrusion depth");
    TopTools_IndexedMapOfShape faces; TopExp::MapShapes(source,TopAbs_FACE,faces);
    require(index<static_cast<uint32_t>(faces.Extent()),"Invalid extrusion face index");
    auto face=TopoDS::Face(faces(index+1));
    BRepAdaptor_Surface surface(face);
    require(surface.GetType()==GeomAbs_Plane,"Face extrusion requires a planar face");
    for(int i=0;i<3;++i) require(std::isfinite(normal[i]),"Invalid extrusion direction");
    gp_Dir direction(normal[0],normal[1],normal[2]);
    require(std::abs(direction.Dot(surface.Plane().Axis().Direction()))>1.0-1e-8,"Extrusion direction must match the face normal");
    TopoDS_Shape profile=face;
    if(start!=0) {
      gp_Trsf shift; shift.SetTranslation(gp_Vec(direction)*start);
      profile=BRepBuilderAPI_Transform(face,shift,true).Shape();
    }
    BRepPrimAPI_MakePrism prism(profile,gp_Vec(direction)*depth,true);
    require(prism.IsDone(),"Exact face extrusion failed");
    auto output=prism.Shape();
    // Pocket travels against the face normal. Do not leave that prism with
    // inward orientation / negative volume when used as an exact cutter.
    require(output.ShapeType()==TopAbs_SOLID,"Face extrusion did not produce a solid");
    auto solid=TopoDS::Solid(output);
    require(BRepLib::OrientClosedSolid(solid),"Face extrusion produced an open solid");
    output=solid;
    validate_result(output);
    GProp_GProps props; BRepGProp::VolumeProperties(output,props);
    require(props.Mass()>0,"Face extrusion must produce a closed positive-volume solid");
    return output;
}
TopoDS_Shape boolean_result(const TopoDS_Shape& a,const TopoDS_Shape& b,int mode,bool refine=false) {
    TopoDS_Shape output;
    { StageTimer timer("boolean build");
    // Float display coordinates of a faceted oblique cap can differ by
    // several native tolerances. Bound local-edit coincidence to 0.00001 mm,
    // rather than generating near-coincident cap slivers. Ordinary Booleans
    // keep their existing default tolerance and face partitioning.
    TopTools_ListOfShape arguments,tools; arguments.Append(a); tools.Append(b);
    auto build=[&](auto& op,const char* error) {
      op.SetArguments(arguments); op.SetTools(tools);
      if(refine) op.SetFuzzyValue(1e-5);
      op.Build(); require(op.IsDone(),error);
      if(refine) op.SimplifyResult(true,true);
      return op.Shape();
    };
    if(mode==0) { BRepAlgoAPI_Fuse op; output=build(op,"Exact union failed"); }
    else if(mode==1) { BRepAlgoAPI_Cut op; output=build(op,"Exact subtract failed"); }
    else { BRepAlgoAPI_Common op; output=build(op,"Exact intersect failed"); }
    }
    validate_result(output);
    return output;
}
}
extern "C" Result* s33d_occt_extrude_face(const char* text,uint32_t index,const double* normal,double depth,double start) noexcept {
  return guarded([&](Result& r) {
    auto output=face_prism(read_brep(text),index,normal,depth,start);
    auto data=data_for(output,true);
    require(data.solid_count==1 && data.volume>0,"Face extrusion must produce a closed positive-volume solid");
    r.shapes.push_back(std::move(data));
  });
}
extern "C" Result* s33d_occt_boolean(const char* left,const char* right,int mode) noexcept {
  return guarded([&](Result& r) {
    auto output=boolean_result(read_brep(left),read_brep(right),mode);
    r.shapes.push_back(data_for(output,true));
  });
}
extern "C" Result* s33d_occt_boolean_refined(const char* left,const char* right,int mode) noexcept {
  return guarded([&](Result& r) {
    // Local wall edits intersect many chordal/coplanar faces. Unify the
    // Boolean result and certify the serialized topology before committing.
    auto output=boolean_result(read_brep(left),read_brep(right),mode,true);
    auto data=data_for(output,true);
    require(BRepCheck_Analyzer(read_brep(data.brep.c_str())).IsValid(),"Persisted Boolean topology is invalid");
    r.shapes.push_back(std::move(data));
  });
}
extern "C" Result* s33d_occt_extrude_face_boolean(const char* text,uint32_t index,const double* normal,double depth,double start,bool cut) noexcept {
  return guarded([&](Result& r) {
    auto source=read_brep(text);
    auto prism=face_prism(source,index,normal,depth,start);
    // Keep full cutter and result validation; omit only the unobserved cutter's
    // serialization, reparsing, triangulation and Rust/C++ buffer round-trip.
    auto output=boolean_result(source,prism,cut?1:0);
    r.shapes.push_back(data_for(output,true));
  });
}
extern "C" Result* s33d_occt_fixture(int kind) noexcept {
  return guarded([&](Result& r) {
    if(kind>=3 && kind<=5) {
      // Spline-encoded imported hardware: a stepped screw, a tube assembled
      // from two half turns, and a deliberately incomplete half-cylinder.
      TopoDS_Shape shape;
      if(kind==3) {
        auto shaft=BRepPrimAPI_MakeCylinder(1.5,10).Shape();
        auto head=BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0,0,10),gp_Dir(0,0,1)),2.8,2).Shape();
        shape=BRepAlgoAPI_Fuse(shaft,head).Shape();
      } else {
        auto outer=BRepPrimAPI_MakeCylinder(4.,10.,3.141592653589793).Shape();
        auto inner=BRepPrimAPI_MakeCylinder(2.,10.).Shape();
        shape=BRepAlgoAPI_Cut(outer,inner).Shape();
        if(kind==4) {
          gp_Trsf turn; turn.SetRotation(gp_Ax1(gp_Pnt(0,0,0),gp_Dir(0,0,1)),3.141592653589793);
          shape=BRepAlgoAPI_Fuse(shape,BRepBuilderAPI_Transform(shape,turn,true).Shape()).Shape();
        }
      }
      r.shapes.push_back(data_for(BRepBuilderAPI_NurbsConvert(shape,true).Shape()));
      return;
    }
    if(kind==2) {
      TColgp_Array2OfPnt poles(1,3,1,3);
      for(int i=1;i<=3;++i) for(int j=1;j<=3;++j) poles.SetValue(i,j,gp_Pnt((i-1)*10,(j-1)*10,(i==2 && j==2)?2:0));
      Handle(Geom_BezierSurface) surface=new Geom_BezierSurface(poles);
      r.shapes.push_back(data_for(BRepBuilderAPI_MakeFace(surface,1e-6).Shape()));
      return;
    }
    TopoDS_Shape shape = kind==0 ? BRepPrimAPI_MakeBox(20,30,40).Shape() : BRepPrimAPI_MakeSphere(10).Shape();
    r.shapes.push_back(data_for(shape));
  });
}
extern "C" void s33d_occt_free(Result* r) noexcept { delete r; }
extern "C" const char* s33d_occt_error(const Result* r) noexcept { return r->error.c_str(); }
extern "C" const char* s33d_occt_text(const Result* r) noexcept { return r->text.c_str(); }
extern "C" size_t s33d_occt_count(const Result* r) noexcept { return r->shapes.size(); }
extern "C" const char* s33d_occt_brep(const Result* r,size_t i) noexcept { return r->shapes[i].brep.c_str(); }
extern "C" size_t s33d_occt_vertices(const Result* r,size_t i) noexcept { return r->shapes[i].positions.size()/3; }
extern "C" size_t s33d_occt_triangles(const Result* r,size_t i) noexcept { return r->shapes[i].faces.size(); }
extern "C" const float* s33d_occt_positions(const Result* r,size_t i) noexcept { return r->shapes[i].positions.data(); }
extern "C" const float* s33d_occt_normals(const Result* r,size_t i) noexcept { return r->shapes[i].normals.data(); }
extern "C" const uint32_t* s33d_occt_indices(const Result* r,size_t i) noexcept { return r->shapes[i].indices.data(); }
extern "C" const uint32_t* s33d_occt_faces(const Result* r,size_t i) noexcept { return r->shapes[i].faces.data(); }
extern "C" const uint8_t* s33d_occt_planar_faces(const Result* r,size_t i) noexcept { return r->shapes[i].planar_faces.data(); }
extern "C" uint32_t s33d_occt_face_count(const Result* r,size_t i) noexcept { return r->shapes[i].face_count; }
extern "C" uint32_t s33d_occt_solid_count(const Result* r,size_t i) noexcept { return r->shapes[i].solid_count; }
